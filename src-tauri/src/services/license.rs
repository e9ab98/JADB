use crate::error::{AppError, AppResult};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tokio::fs;
use uuid::Uuid;

pub const FEATURE_REPORT_EXPORT: &str = "apk_report_export";
pub const FEATURE_SIGNING_V31: &str = "signing_v31";
pub const FEATURE_ADB_MULTI_DEVICE: &str = "adb_multi_device";

// Replace this Base64URL Ed25519 public key before producing release licenses.
// The admin CLI prints the matching value when `keygen` is run.
pub const LICENSE_PUBLIC_KEY: &str = "HssE7V2slGAOUTNY7pe6pRwoYHy_YQ1_ZJ1dgD39KIY";

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
}

impl LicenseStatus {
    fn inactive(state: &str, device_id: String, message: Option<String>) -> Self {
        Self { state: state.into(), device_id, edition: "free".into(), license_id: None,
            licensed_to: None, expires_at: None, perpetual: false, features: vec![], message }
    }
    pub fn is_active(&self) -> bool { self.state == "active" }
    pub fn has_feature(&self, feature: &str) -> bool {
        self.is_active() && self.features.iter().any(|value| value == feature)
    }
}

#[derive(Clone)]
pub struct LicenseService { public_key: VerifyingKey }

impl LicenseService {
    pub fn new() -> Self {
        let bytes = URL_SAFE_NO_PAD.decode(LICENSE_PUBLIC_KEY).expect("invalid embedded license public key");
        let bytes: [u8; 32] = bytes.try_into().expect("license public key must be 32 bytes");
        Self { public_key: VerifyingKey::from_bytes(&bytes).expect("invalid Ed25519 license public key") }
    }

    fn license_path(&self, app: &AppHandle) -> AppResult<PathBuf> {
        Ok(app.path().app_data_dir().map_err(|e| AppError::Config(e.to_string()))?.join("license.json"))
    }

    pub async fn device_id(&self, app: &AppHandle) -> AppResult<String> {
        let dir = app.path().app_data_dir().map_err(|e| AppError::Config(e.to_string()))?;
        let source = stable_machine_id().await.unwrap_or_else(|| String::new());
        let raw = if source.is_empty() { fallback_install_id(&dir).await? } else { source };
        let digest = Sha256::digest(format!("jadb:v1:{}:{raw}", std::env::consts::OS));
        let code = hex::encode_upper(&digest[..8]);
        Ok(format!("JADB-{}-{}-{}-{}", &code[0..4], &code[4..8], &code[8..12], &code[12..16]))
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

    async fn evaluate(&self, app: &AppHandle, token: &str) -> AppResult<LicenseStatus> {
        let device_id = self.device_id(app).await?;
        let payload = match self.verify_token(token) {
            Ok(payload) => payload,
            Err(error) => return Ok(LicenseStatus::inactive("invalid", device_id, Some(error.to_string()))),
        };
        if payload.version != 1 || payload.product != "jadb" || payload.edition != "vip" {
            return Ok(LicenseStatus::inactive("invalid", device_id, Some("license is not issued for JADB VIP".into())));
        }
        if payload.device_id != device_id {
            return Ok(LicenseStatus::inactive("deviceMismatch", device_id, Some("license belongs to another device".into())));
        }
        if !payload.perpetual && payload.expires_at.as_ref().map(|date| date < &Utc::now()).unwrap_or(true) {
            return Ok(LicenseStatus { state: "expired".into(), device_id, edition: "free".into(),
                license_id: Some(payload.license_id), licensed_to: payload.licensed_to,
                expires_at: payload.expires_at, perpetual: false, features: vec![], message: Some("license has expired".into()) });
        }
        Ok(LicenseStatus { state: "active".into(), device_id, edition: "vip".into(),
            license_id: Some(payload.license_id), licensed_to: payload.licensed_to,
            expires_at: payload.expires_at, perpetual: payload.perpetual, features: payload.features, message: None })
    }

    pub async fn status(&self, app: &AppHandle) -> AppResult<LicenseStatus> {
        let path = self.license_path(app)?;
        let device_id = self.device_id(app).await?;
        if !path.exists() { return Ok(LicenseStatus::inactive("unlicensed", device_id, None)); }
        let bytes = fs::read(&path).await?;
        let stored: StoredLicense = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) => return Ok(LicenseStatus::inactive("invalid", device_id, Some("stored license is damaged".into()))),
        };
        self.evaluate(app, &stored.token).await
    }

    pub async fn activate(&self, app: &AppHandle, token: &str) -> AppResult<LicenseStatus> {
        let status = self.evaluate(app, token).await?;
        if !status.is_active() { return Err(AppError::InvalidInput(status.message.clone().unwrap_or(status.state.clone()))); }
        let path = self.license_path(app)?;
        if let Some(parent) = path.parent() { fs::create_dir_all(parent).await?; }
        fs::write(&path, serde_json::to_vec_pretty(&StoredLicense { token: token.trim().into() }).map_err(|e| AppError::Config(e.to_string()))?).await?;
        set_private_permissions(&path)?;
        Ok(status)
    }

    pub async fn remove(&self, app: &AppHandle) -> AppResult<LicenseStatus> {
        let path = self.license_path(app)?;
        if path.exists() { fs::remove_file(path).await?; }
        self.status(app).await
    }

    pub async fn require_feature(&self, app: &AppHandle, feature: &str) -> AppResult<()> {
        if self.status(app).await?.has_feature(feature) { Ok(()) }
        else { Err(AppError::InvalidInput(format!("VIP_REQUIRED:{feature}"))) }
    }
}

#[derive(Serialize, Deserialize)] struct StoredLicense { token: String }

async fn fallback_install_id(dir: &Path) -> AppResult<String> {
    let path = dir.join("installation-id");
    if path.exists() { return Ok(String::from_utf8_lossy(&fs::read(path).await?).trim().into()); }
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
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?; }
    Ok(())
}
