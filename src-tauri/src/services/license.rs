use crate::config::settings::{read as read_settings, Settings};
use crate::error::{AppError, AppResult};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::fs;
use uuid::Uuid;

pub const FEATURE_ALL: &str = "all";
pub const FEATURE_REPORT_EXPORT: &str = "apk_report_export";
pub const FEATURE_SIGNING_V31: &str = "signing_v31";
pub const FEATURE_ADB_MULTI_DEVICE: &str = "adb_multi_device";
pub const FEATURE_ADB_BATCH_INSTALL: &str = "adb_batch_install";

// Replace this Base64URL Ed25519 public key before producing release licenses.
// The admin CLI prints the matching value when `keygen` is run.
pub const LICENSE_PUBLIC_KEY: &str = "HssE7V2slGAOUTNY7pe6pRwoYHy_YQ1_ZJ1dgD39KIY";

/// HTTP timeout when talking to the remote license-server.
const ONLINE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicensePayload {
    pub version: u8,
    pub license_id: String,
    pub product: String,
    pub edition: String,
    pub device_id: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub perpetual: bool,
    pub features: Vec<String>,
    pub licensed_to: Option<String>,
}

/// 最终对外的状态。
///
/// `mode` 说明当前结果是怎么得出的：
/// - `offline`       — 没有配 server URL，按纯本地 token 验签得到
/// - `online`        — 调了 server 一次通过，state 完全以 server 为准
/// - `offline_degraded` — 配了 server URL 但请求失败，按本地结果兜底
///
/// `last_verified_at` 是最近一次**成功在线校验**的时间。`offline` 模式下为 None。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseStatus {
    pub state: String,
    pub device_id: String,
    pub edition: String,
    pub license_id: Option<String>,
    pub licensed_to: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub perpetual: bool,
    pub features: Vec<String>,
    pub message: Option<String>,
    /// `"online" | "offline" | "offline_degraded"`
    #[serde(default = "default_mode")]
    pub mode: String,
    /// 服务端校验成功时间（online / offline_degraded 模式都有；纯离线为 None）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_verified_at: Option<DateTime<Utc>>,
    /// 是否可以「替换绑定」：在线模式下 server 告知本 license 已绑到其他机器时为 true。
    /// 离线 / 离线降级 / 不存在 license 时一律为 false。
    #[serde(default)]
    pub can_rebind: bool,
    /// Server 当前绑定的 deviceId（仅当 can_rebind=true 有值）。
    /// UI 用它提示「license 已绑到机器 X」。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bound_device_id: Option<String>,
}

fn default_mode() -> String { "offline".into() }

impl LicenseStatus {
    fn inactive(state: &str, device_id: String, message: Option<String>) -> Self {
        Self {
            state: state.into(),
            device_id,
            edition: "free".into(),
            license_id: None,
            licensed_to: None,
            expires_at: None,
            perpetual: false,
            features: vec![],
            message,
            mode: default_mode(),
            last_verified_at: None,
            can_rebind: false,
            bound_device_id: None,
        }
    }

    /// 构造一个「已绑到其他设备，可替换」的 deviceMismatch 状态。
    fn device_mismatch_rebindable(device_id: String, bound: String) -> Self {
        Self {
            state: "deviceMismatch".into(),
            device_id,
            edition: "free".into(),
            license_id: None,
            licensed_to: None,
            expires_at: None,
            perpetual: false,
            features: vec![],
            message: Some(format!("license 已绑定到 {bound}，需要在本机替换绑定")),
            mode: "online".into(),
            last_verified_at: Some(Utc::now()),
            can_rebind: true,
            bound_device_id: Some(bound),
        }
    }
    pub fn is_active(&self) -> bool { self.state == "active" }
    pub fn has_feature(&self, feature: &str) -> bool {
        self.is_active() && (self.features.iter().any(|value| value == feature)
            || self.features.iter().any(|value| value == FEATURE_ALL))
    }
}

/// Server 端 `/verify` 响应字段中我们关心的部分。
///
/// 注意：`reasons` 里可能包含 `"binding_conflict"` —— 这是新增的设备绑定冲突原因，
/// 与 `"device_mismatch"`（payload.device_id 不匹配）含义不同：
/// - `device_mismatch`：客户端提供的 deviceId 与签发时的 deviceId 不一致（payload 层）
/// - `binding_conflict`：该 license 已被 server 绑定到另一台机器（runtime 层）
///
/// client 端用 `needs_replacement` 这个语义化字段判断，避免解析 reasons 字符串。
#[derive(Clone, Debug, Deserialize)]
struct VerifyResponse {
    valid: bool,
    #[serde(default)]
    revoked: bool,
    #[serde(default)]
    expired: bool,
    #[serde(default)]
    device_match: Option<bool>,
    #[serde(default)]
    reasons: Vec<String>,
    /// server 端是否要求本客户端「替换绑定」（即：license 已被绑到别的机器）
    #[serde(default)]
    needs_replacement: bool,
    /// server 当前绑定的 deviceId（仅当 needs_replacement=true 有意义）
    #[serde(default)]
    current_bound_device_id: Option<String>,
    // 注：boundAt 也从 server 返回，但 client 端只关心 needs_replacement / current_bound_device_id。
    // 该字段保留在 server 响应里，便于将来给 UI 显示「绑定时间」。
    #[serde(default)]
    #[allow(dead_code)]
    bound_at: Option<DateTime<Utc>>,
}

#[derive(Clone)]
pub struct LicenseService {
    public_key: VerifyingKey,
    http: reqwest::Client,
}

impl LicenseService {
    pub fn new() -> Self {
        let bytes = URL_SAFE_NO_PAD.decode(LICENSE_PUBLIC_KEY).expect("invalid embedded license public key");
        let bytes: [u8; 32] = bytes.try_into().expect("license public key must be 32 bytes");
        let http = reqwest::Client::builder()
            .timeout(ONLINE_TIMEOUT)
            .connect_timeout(Duration::from_secs(3))
            .build()
            .expect("failed to build reqwest client");
        Self {
            public_key: VerifyingKey::from_bytes(&bytes).expect("invalid Ed25519 license public key"),
            http,
        }
    }

    fn license_path(&self, app: &AppHandle) -> AppResult<PathBuf> {
        Ok(app.path().app_data_dir().map_err(|e| AppError::Config(e.to_string()))?.join("license.json"))
    }

    async fn load_settings(&self, app: &AppHandle) -> AppResult<Settings> {
        let dir = app.path().app_data_dir().map_err(|e| AppError::Config(e.to_string()))?;
        read_settings(&dir).await
    }

    /// 取当前生效的 server URL；未配置返回 `None`。
    pub async fn server_url(&self, app: &AppHandle) -> AppResult<Option<String>> {
        let s = self.load_settings(app).await?;
        Ok(s.license_server_url.and_then(normalize_server_url))
    }

    pub async fn device_id(&self, app: &AppHandle) -> AppResult<String> {
        let dir = app.path().app_data_dir().map_err(|e| AppError::Config(e.to_string()))?;
        let source = stable_machine_id().await.unwrap_or_else(|| String::new());
        let raw = if source.is_empty() { fallback_install_id(&dir).await? } else { source };
        let digest = Sha256::digest(format!("jadb:v1:{}:{raw}", std::env::consts::OS));
        let code = hex::encode_upper(&digest[..8]);
        Ok(format!("JADB-{}-{}-{}-{}", &code[0..4], &code[4..8], &code[8..12], &code[12..16]))
    }

    /// 仅做 Ed25519 签名校验。不查 payload 业务字段、不查 deviceId、不查 server。
    fn verify_signature_only(&self, token: &str) -> AppResult<LicensePayload> {
        self.verify_token(token)
    }

    /// 校验 payload 的业务字段（version/product/edition），不看 device 也不看时间。
    fn validate_payload_business(&self, payload: &LicensePayload) -> AppResult<()> {
        if payload.version != 1 || payload.product != "jadb" || payload.edition != "vip" {
            return Err(AppError::InvalidInput("license is not issued for JADB VIP".into()));
        }
        Ok(())
    }

    fn verify_token(&self, token: &str) -> AppResult<LicensePayload> {
        let parts: Vec<&str> = token.trim().split('.').collect();
        if parts.len() != 3 || parts[0] != "JADB1" {
            return Err(AppError::InvalidInput("invalid license format".into()));
        }
        let payload_bytes = URL_SAFE_NO_PAD.decode(parts[1]).map_err(|_| AppError::InvalidInput("invalid license payload encoding".into()))?;
        let signature_bytes = URL_SAFE_NO_PAD.decode(parts[2]).map_err(|_| AppError::InvalidInput("invalid license signature encoding".into()))?;
        let signature = Signature::from_slice(&signature_bytes).map_err(|_| AppError::InvalidInput("invalid license signature".into()))?;
        self.public_key.verify(parts[1].as_bytes(), &signature).map_err(|_| AppError::InvalidInput("license signature verification failed".into()))?;
        serde_json::from_slice(&payload_bytes).map_err(|_| AppError::InvalidInput("invalid license payload".into()))
    }

    /// HTTP 调 `/api/v1/license/verify`。
    ///
    /// 现在**会传**本机 deviceId：server 端会按 `bound_devices` 决定：
    /// - 本机已在列表 → valid
    /// - 列表为空 → 自动绑定本机 → valid
    /// - 列表非空且不含本机 → 返回 `needs_replacement`，client 进入替换流程
    ///
    /// 这是「在线模式单设备独占」防滥用的核心。彻底替换了之前
    /// 「故意不传 deviceId 让一个 token 多机共用」的设计。
    async fn verify_remote(
        &self,
        server_url: &str,
        token: &str,
        device_id: &str,
    ) -> AppResult<VerifyResponse> {
        let url = format!("{}/api/v1/license/verify", server_url.trim_end_matches('/'));
        let resp = self
            .http
            .post(&url)
            .json(&serde_json::json!({
                "token": token,
                "deviceId": device_id,
            }))
            .send()
            .await
            .map_err(|e| AppError::Config(format!("调用 license-server 失败：{e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::Config(format!("license-server 返回 {status}: {body}")));
        }
        resp.json::<VerifyResponse>()
            .await
            .map_err(|e| AppError::Config(format!("解析 license-server 响应失败：{e}")))
    }

    /// HTTP 调 `/api/v1/license/bind`：把 license 绑定到本机（替换式）。
    ///
    /// 不写本地、不修改 evaluate 流——纯粹告诉 server「我的机器现在要用了」。
    /// server 原子清空 `bound_devices` 后加入新 device。
    async fn rebind_remote(
        &self,
        server_url: &str,
        token: &str,
        device_id: &str,
    ) -> AppResult<()> {
        let url = format!("{}/api/v1/license/bind", server_url.trim_end_matches('/'));
        let resp = self
            .http
            .post(&url)
            .json(&serde_json::json!({
                "token": token,
                "deviceId": device_id,
            }))
            .send()
            .await
            .map_err(|e| AppError::Config(format!("调用 license-server bind 失败：{e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::Config(format!(
                "license-server bind 返回 {status}: {body}"
            )));
        }
        Ok(())
    }

    /// 计算 status 的核心：先验签 + 业务字段，再按模式决定是否校验设备 / 是否问 server。
    ///
    /// 三种模式下的设备检查策略（在线模式会带 deviceId 上 server，做「单设备独占」绑定）：
    /// - 离线（无 server URL）   → 本机 deviceId 与 payload.device_id 严格匹配（payload 层）
    /// - 在线（配了 server URL 且可达）→ server 校验吊销/过期 + 绑定本机到 bound_devices
    ///   - 若 server 说 needs_replacement → 返回 deviceMismatch (canRebind=true)
    /// - 降级（在线挂了）       → 本机 deviceId 严格匹配（payload 层，安全降级）
    ///
    /// `force_online` = false 时若 server 不可达会静默降级；true 时失败直接报错（用于主动同步）。
    async fn evaluate(&self, app: &AppHandle, token: &str, force_online: bool) -> AppResult<LicenseStatus> {
        let device_id = self.device_id(app).await?;

        // 1) 签名 + payload 业务字段（无论哪种模式都必须过）
        let payload = match (|| -> AppResult<LicensePayload> {
            let p = self.verify_signature_only(token)?;
            self.validate_payload_business(&p)?;
            Ok(p)
        })() {
            Ok(p) => p,
            Err(e) => return Ok(LicenseStatus::inactive("invalid", device_id, Some(e.to_string()))),
        };

        // 2) 过期检查（本地权威，无论哪种模式都用）
        let offline_expired = !payload.perpetual
            && payload.expires_at.as_ref().map(|d| d < &Utc::now()).unwrap_or(true);

        // 3) 按模式走
        let server_url = self.server_url(app).await?;
        match server_url {
            None => {
                // ===== 纯离线：严格设备匹配 =====
                // 盲 license（payload.device_id 为空）在这里直接拒绝：
                // 它本质上要求在线校验（首次 verify 自动绑定），离线模式无法满足。
                if payload.device_id.is_empty() {
                    return Ok(LicenseStatus::inactive(
                        "noDeviceBinding",
                        device_id,
                        Some(
                            "此 license 未绑定设备码，仅限在线校验。请在设置中配置 license server URL"
                                .into(),
                        ),
                    ));
                }
                if payload.device_id != device_id {
                    return Ok(LicenseStatus::inactive(
                        "deviceMismatch",
                        device_id,
                        Some("license belongs to another device".into()),
                    ));
                }
                if offline_expired {
                    Ok(expired_status(payload, device_id, "license has expired"))
                } else {
                    Ok(active_offline(payload, device_id, None))
                }
            }
            Some(url) => {
                // ===== 在线：传本机 deviceId，让 server 做「单设备独占」绑定 =====
                match self.verify_remote(&url, token, &device_id).await {
                    Ok(r) => {
                        // 绑定冲突：server 说本 license 已绑到其他机器。
                        // 直接告诉用户「需要替换绑定」；不要 fallback 到 active。
                        if r.needs_replacement {
                            let bound = r
                                .current_bound_device_id
                                .clone()
                                .unwrap_or_else(|| "<未知>".into());
                            return Ok(LicenseStatus::device_mismatch_rebindable(
                                device_id, bound,
                            ));
                        }
                        if r.revoked {
                            Ok(revoked_status(payload, device_id, "license has been revoked by server"))
                        } else if r.expired || offline_expired {
                            Ok(expired_status(payload, device_id, "license has expired"))
                        } else if r.valid {
                            // 重要：payload.device_id 字段不参与在线模式的设备判定，
                            // server 端按 bound_devices 判定。
                            Ok(LicenseStatus {
                                state: "active".into(),
                                device_id,
                                edition: "vip".into(),
                                license_id: Some(payload.license_id),
                                licensed_to: payload.licensed_to,
                                expires_at: payload.expires_at,
                                perpetual: payload.perpetual,
                                features: payload.features,
                                message: Some(format!("online verified @ {url} (bound)")),
                                mode: "online".into(),
                                last_verified_at: Some(Utc::now()),
                                can_rebind: false,
                                bound_device_id: None,
                            })
                        } else {
                            Ok(LicenseStatus::inactive(
                                "serverRejected",
                                device_id,
                                Some(format!("server rejected: {:?}", r.reasons)),
                            ))
                        }
                    }
                    Err(e) => {
                        if force_online {
                            // 主动同步：把错误向上抛，UI 弹 toast
                            return Err(e);
                        }
                        // ===== 降级：仍然按严格设备匹配兜底 =====
                        // 盲 license 在降级分支也直接拒绝 —— 没设备码没法兜底匹配。
                        if payload.device_id.is_empty() {
                            return Ok(LicenseStatus::inactive(
                                "noDeviceBinding",
                                device_id,
                                Some(format!(
                                    "此 license 未绑定设备码（服务器也不可达）: {e}"
                                )),
                            ));
                        }
                        if payload.device_id != device_id {
                            return Ok(LicenseStatus::inactive(
                                "deviceMismatch",
                                device_id,
                                Some(format!(
                                    "license belongs to another device (offline degraded): {e}"
                                )),
                            ));
                        }
                        if offline_expired {
                            Ok(expired_status(
                                payload,
                                device_id,
                                &format!("离线降级（服务器不可达）: {e}"),
                            ))
                        } else {
                            let mut s = active_offline(
                                payload,
                                device_id,
                                Some(format!("离线降级：服务器不可达：{e}")),
                            );
                            s.mode = "offline_degraded".into();
                            Ok(s)
                        }
                    }
                }
            }
        }
    }

    pub async fn status(&self, app: &AppHandle) -> AppResult<LicenseStatus> {
        let path = self.license_path(app)?;
        let device_id = self.device_id(app).await?;
        if !path.exists() {
            return Ok(LicenseStatus::inactive("unlicensed", device_id, None));
        }
        let bytes = fs::read(&path).await?;
        let stored: StoredLicense = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) => return Ok(LicenseStatus::inactive("invalid", device_id, Some("stored license is damaged".into()))),
        };
        self.evaluate(app, &stored.token, false).await
    }

    /// 主动同步：强制在线校验，失败抛错（不静默降级）。
    pub async fn refresh(&self, app: &AppHandle) -> AppResult<LicenseStatus> {
        let path = self.license_path(app)?;
        let device_id = self.device_id(app).await?;
        if !path.exists() {
            return Ok(LicenseStatus::inactive("unlicensed", device_id, None));
        }
        let bytes = fs::read(&path).await?;
        let stored: StoredLicense = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) => return Ok(LicenseStatus::inactive("invalid", device_id, Some("stored license is damaged".into()))),
        };
        self.evaluate(app, &stored.token, true).await
    }

    pub async fn activate(&self, app: &AppHandle, token: &str) -> AppResult<LicenseStatus> {
        let status = self.evaluate(app, token, false).await?;
        if !status.is_active() {
            return Err(AppError::InvalidInput(status.message.clone().unwrap_or(status.state.clone())));
        }
        let path = self.license_path(app)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&path, serde_json::to_vec_pretty(&StoredLicense { token: token.trim().into() }).map_err(|e| AppError::Config(e.to_string()))?).await?;
        set_private_permissions(&path)?;
        Ok(status)
    }

    pub async fn remove(&self, app: &AppHandle) -> AppResult<LicenseStatus> {
        let path = self.license_path(app)?;
        if path.exists() {
            fs::remove_file(path).await?;
        }
        self.status(app).await
    }

    /// 不写本地，仅做一次在线校验（公开给前端「先预览」使用）。
    /// 现在与 `evaluate` 的在线分支保持一致：传本机 deviceId、按 server 的 binding 判断。
    /// 任何一步失败都会返回错误结果（不会降级）。
    pub async fn verify_remote_only(&self, app: &AppHandle, token: &str) -> AppResult<LicenseStatus> {
        let device_id = self.device_id(app).await?;
        let server_url = self.server_url(app).await?
            .ok_or_else(|| AppError::InvalidInput("未配置 license server URL".into()))?;
        let payload = self.verify_signature_only(token)?;
        self.validate_payload_business(&payload)?;
        let r = self.verify_remote(&server_url, token, &device_id).await?;
        // 与 evaluate 在线分支对齐：needs_replacement 优先于其他失效原因
        if r.needs_replacement {
            let bound = r
                .current_bound_device_id
                .clone()
                .unwrap_or_else(|| "<未知>".into());
            return Ok(LicenseStatus::device_mismatch_rebindable(device_id, bound));
        }
        if r.revoked {
            Ok(revoked_status(payload, device_id, "license has been revoked by server"))
        } else if r.expired {
            Ok(expired_status(payload, device_id, "license has expired"))
        } else if matches!(r.device_match, Some(false)) {
            // payload.device_id 与本机不同（签发机器 vs 当前机器）
            Ok(LicenseStatus::inactive("deviceMismatch", device_id, Some("server says device mismatch".into())))
        } else if r.valid {
            Ok(LicenseStatus {
                state: "active".into(),
                device_id,
                edition: "vip".into(),
                license_id: Some(payload.license_id),
                licensed_to: payload.licensed_to,
                expires_at: payload.expires_at,
                perpetual: payload.perpetual,
                features: payload.features,
                message: Some(format!("online verified @ {server_url}")),
                mode: "online".into(),
                last_verified_at: Some(Utc::now()),
                can_rebind: false,
                bound_device_id: None,
            })
        } else {
            Ok(LicenseStatus::inactive("serverRejected", device_id, Some(format!("{:?}", r.reasons))))
        }
    }

    /// 在线模式「替换绑定」：调 server `/api/v1/license/bind`，再重跑一次 evaluate。
    ///
    /// **重要前置条件**：调用方必须先把当前 token 写到本地（`activate` 已经做过）。
    /// 否则这里读不到 token，会返回错误。
    ///
    /// 行为：
    /// 1. 读 license.json 里的 token（必须存在）
    /// 2. 调 server rebind，传本机 deviceId
    /// 3. 重新跑 evaluate 让本机进入 active 状态（同时把旧机器踢到 can_rebind=true）
    pub async fn rebind_online(&self, app: &AppHandle) -> AppResult<LicenseStatus> {
        let path = self.license_path(app)?;
        if !path.exists() {
            return Err(AppError::InvalidInput(
                "本机尚未激活 license，无法执行替换绑定".into(),
            ));
        }
        let bytes = fs::read(&path).await?;
        let stored: StoredLicense = serde_json::from_slice(&bytes)
            .map_err(|_| AppError::InvalidInput("stored license is damaged".into()))?;
        let server_url = self
            .server_url(app)
            .await?
            .ok_or_else(|| AppError::InvalidInput("未配置 license server URL".into()))?;
        let device_id = self.device_id(app).await?;
        self.rebind_remote(&server_url, &stored.token, &device_id)
            .await?;
        // 替换完后再跑一次 evaluate：现在本机已绑，应该 valid。
        // 用 force_online=true 避免静默降级（如果替换后又连不上 server 就报错给用户看）。
        self.evaluate(app, &stored.token, true).await
    }

    pub async fn require_feature(&self, app: &AppHandle, feature: &str) -> AppResult<()> {
        if self.status(app).await?.has_feature(feature) {
            Ok(())
        } else {
            Err(AppError::InvalidInput(format!("VIP_REQUIRED:{feature}")))
        }
    }
}

// ---------- 构造器辅助 ----------

fn active_offline(payload: LicensePayload, device_id: String, message: Option<String>) -> LicenseStatus {
    LicenseStatus {
        state: "active".into(),
        device_id,
        edition: "vip".into(),
        license_id: Some(payload.license_id),
        licensed_to: payload.licensed_to,
        expires_at: payload.expires_at,
        perpetual: payload.perpetual,
        features: payload.features,
        message,
        mode: "offline".into(),
        last_verified_at: None,
        can_rebind: false,
        bound_device_id: None,
    }
}

fn expired_status(payload: LicensePayload, device_id: String, message: &str) -> LicenseStatus {
    LicenseStatus {
        state: "expired".into(),
        device_id,
        edition: "free".into(),
        license_id: Some(payload.license_id),
        licensed_to: payload.licensed_to,
        expires_at: payload.expires_at,
        perpetual: false,
        features: vec![],
        message: Some(message.into()),
        mode: "offline".into(),
        last_verified_at: None,
        can_rebind: false,
        bound_device_id: None,
    }
}

fn revoked_status(payload: LicensePayload, device_id: String, message: &str) -> LicenseStatus {
    LicenseStatus {
        state: "revoked".into(),
        device_id,
        edition: "free".into(),
        license_id: Some(payload.license_id),
        licensed_to: payload.licensed_to,
        expires_at: payload.expires_at,
        perpetual: false,
        features: vec![],
        message: Some(message.into()),
        mode: "online".into(),
        last_verified_at: Some(Utc::now()),
        can_rebind: false,
        bound_device_id: None,
    }
}

/// 规范化 server URL：去尾部斜杠、过滤无效值。
fn normalize_server_url(raw: String) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let without_slash = trimmed.trim_end_matches('/').to_string();
    if without_slash.starts_with("http://") || without_slash.starts_with("https://") {
        Some(without_slash)
    } else {
        None
    }
}

#[derive(Serialize, Deserialize)]
struct StoredLicense { token: String }

async fn fallback_install_id(dir: &Path) -> AppResult<String> {
    let path = dir.join("installation-id");
    if path.exists() {
        return Ok(String::from_utf8_lossy(&fs::read(path).await?).trim().into());
    }
    fs::create_dir_all(dir).await?;
    let id = Uuid::new_v4().to_string();
    fs::write(&path, id.as_bytes()).await?;
    set_private_permissions(&path)?;
    Ok(id)
}

async fn stable_machine_id() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let output = tokio::process::Command::new("ioreg").args(["-rd1", "-c", "IOPlatformExpertDevice"]).output().await.ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        return text.lines().find(|line| line.contains("IOPlatformUUID"))
            .and_then(|line| line.split('=').nth(1)).map(|v| v.trim().trim_matches('"').to_string());
    }
    #[cfg(target_os = "windows")]
    {
        let output = tokio::process::Command::new("reg").args(["query", r"HKLM\SOFTWARE\Microsoft\Cryptography", "/v", "MachineGuid"]).output().await.ok()?;
        return String::from_utf8_lossy(&output.stdout).lines().find(|line| line.contains("MachineGuid"))
            .and_then(|line| line.split_whitespace().last()).map(str::to_string);
    }
    #[allow(unreachable_code)] None
}

fn set_private_permissions(path: &Path) -> AppResult<()> {
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
