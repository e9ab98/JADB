use crate::commands::signatures::SignatureConfig;
use crate::config::settings::Settings;
use crate::error::{AppError, AppResult};
use crate::progress;
use crate::services::lineage_manager;
use crate::services::signature_manager;
use crate::services::task_registry::TaskRegistry;
use crate::services::java_runtime;
use crate::services::tool_manager::{self, BuildToolsPaths};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

pub const KEYSTORE_PASSWORD_ENV: &str = "JADB_APKSIGNER_KS_PASS";
pub const KEY_PASSWORD_ENV: &str = "JADB_APKSIGNER_KEY_PASS";
// Separate env names for rotation so the old/new signer passwords cannot
// accidentally clobber each other when both signers are configured.
pub const KEYSTORE_PASSWORD_ENV_OLD: &str = "JADB_APKSIGNER_KS_PASS_OLD";
pub const KEY_PASSWORD_ENV_OLD: &str = "JADB_APKSIGNER_KEY_PASS_OLD";

/// Minimum SDK version required for V3.1 rotation lineage verification.
pub const ROTATION_MIN_SDK_VERSION: u32 = 33;

#[derive(Serialize, Clone, Debug)]
pub struct TaskHandle {
    pub task_id: String,
    pub kind: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SigningSchemes {
    pub v1: bool,
    pub v2: bool,
    pub v3: bool,
    pub v4: bool,
}

impl Default for SigningSchemes {
    fn default() -> Self {
        Self {
            v1: true,
            v2: true,
            v3: true,
            v4: true,
        }
    }
}

impl SigningSchemes {
    pub fn validate(self) -> AppResult<()> {
        if !self.v1 && !self.v2 && !self.v3 && !self.v4 {
            return Err(AppError::InvalidInput(
                "At least one signing scheme must be enabled".into(),
            ));
        }
        if self.v4 && !self.v2 && !self.v3 {
            return Err(AppError::InvalidInput(
                "V4 signing requires at least V2 or V3 signing to be enabled".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct SigningOutput {
    pub apk_path: String,
    pub idsig_path: Option<String>,
}

pub fn resolve_build_tools(settings: &Settings) -> AppResult<BuildToolsPaths> {
    let configured = settings
        .android_build_tools_dir
        .as_deref()
        .ok_or_else(|| AppError::ToolMissing("Android Build-Tools".into()))?;
    tool_manager::resolve_android_build_tools_dir(Path::new(configured)).map_err(|error| {
        AppError::Config(format!(
            "Android Build-Tools 配置无效，请前往设置 → 工具链重新配置: {error}"
        ))
    })
}

pub fn build_zipalign_args(input: &str, output: &str) -> Vec<String> {
    vec![
        "-f".into(),
        "-p".into(),
        "4".into(),
        input.into(),
        output.into(),
    ]
}

pub fn build_apksigner_args(
    apksigner_jar: &str,
    aligned_apk: &str,
    output_apk: &str,
    keystore: &str,
    alias: &str,
    schemes: SigningSchemes,
) -> Vec<String> {
    vec![
        "-Xmx1024M".into(),
        "-jar".into(),
        apksigner_jar.into(),
        "sign".into(),
        "--ks".into(),
        keystore.into(),
        "--ks-key-alias".into(),
        alias.into(),
        "--ks-pass".into(),
        format!("env:{KEYSTORE_PASSWORD_ENV}"),
        "--key-pass".into(),
        format!("env:{KEY_PASSWORD_ENV}"),
        "--v1-signing-enabled".into(),
        bool_arg(schemes.v1).into(),
        "--v2-signing-enabled".into(),
        bool_arg(schemes.v2).into(),
        "--v3-signing-enabled".into(),
        bool_arg(schemes.v3).into(),
        "--v4-signing-enabled".into(),
        bool_arg(schemes.v4).into(),
        "--out".into(),
        output_apk.into(),
        aligned_apk.into(),
    ]
}

fn bool_arg(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

#[allow(clippy::too_many_arguments)]
pub fn build_apksigner_rotation_args(
    apksigner_jar: &str,
    aligned_apk: &str,
    output_apk: &str,
    old_keystore: &str,
    old_alias: &str,
    new_keystore: &str,
    new_alias: &str,
    lineage: &str,
    v4_enabled: bool,
) -> Vec<String> {
    vec![
        "-Xmx1024M".into(),
        "-jar".into(),
        apksigner_jar.into(),
        "sign".into(),
        "--ks".into(),
        old_keystore.into(),
        "--ks-key-alias".into(),
        old_alias.into(),
        "--ks-pass".into(),
        format!("env:{KEYSTORE_PASSWORD_ENV_OLD}"),
        "--key-pass".into(),
        format!("env:{KEY_PASSWORD_ENV_OLD}"),
        "--next-signer".into(),
        "--ks".into(),
        new_keystore.into(),
        "--ks-key-alias".into(),
        new_alias.into(),
        "--ks-pass".into(),
        format!("env:{KEYSTORE_PASSWORD_ENV}"),
        "--key-pass".into(),
        format!("env:{KEY_PASSWORD_ENV}"),
        "--lineage".into(),
        lineage.into(),
        "--rotation-min-sdk-version".into(),
        ROTATION_MIN_SDK_VERSION.to_string(),
        "--v1-signing-enabled".into(),
        "true".into(),
        "--v2-signing-enabled".into(),
        "true".into(),
        "--v3-signing-enabled".into(),
        "true".into(),
        "--v4-signing-enabled".into(),
        bool_arg(v4_enabled).into(),
        "--out".into(),
        output_apk.into(),
        aligned_apk.into(),
    ]
}

pub async fn run_signing(
    app: &AppHandle,
    task_id: &str,
    build_tools: &BuildToolsPaths,
    apk: &str,
    signature: &SignatureConfig,
    schemes: SigningSchemes,
    progress_base: f32,
    settings: &Settings,
    app_data_dir: &Path,
) -> AppResult<Option<SigningOutput>> {
    schemes.validate()?;
    let input = PathBuf::from(apk);
    if !input.is_file() {
        return Err(AppError::InvalidInput(format!("APK 文件不存在: {apk}")));
    }
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("output");
    let temporary_aligned = parent.join(format!(".{stem}-{task_id}-aligned.apk"));
    let temporary_signed = parent.join(format!(".{stem}-{task_id}-signed.apk"));
    let temporary_idsig = idsig_path(&temporary_signed);
    let final_apk = PathBuf::from(format_apk_signed_path(apk));
    let final_idsig = idsig_path(&final_apk);
    let _cleanup = scopeguard::guard(
        vec![
            temporary_aligned.clone(),
            temporary_signed.clone(),
            temporary_idsig.clone(),
        ],
        |paths| {
            for path in paths {
                let _ = std::fs::remove_file(path);
            }
        },
    );
    let _ = tokio::fs::remove_file(&temporary_aligned).await;
    let _ = tokio::fs::remove_file(&temporary_signed).await;
    let _ = tokio::fs::remove_file(&temporary_idsig).await;

    progress::emit_progress(app, task_id, progress_base, "APK 对齐中");
    let mut zipalign = Command::new(&build_tools.zipalign);
    zipalign.args(build_zipalign_args(
        &input.to_string_lossy(),
        &temporary_aligned.to_string_lossy(),
    ));
    if !run_streamed_command(app, task_id, "zipalign", zipalign).await? {
        return Ok(None);
    }
    if !temporary_aligned.is_file() {
        progress::emit_error(app, task_id, "zipalign 未生成对齐后的 APK");
        return Ok(None);
    }

    let sign_percent = progress_base + (100.0 - progress_base) * 0.45;
    progress::emit_progress(app, task_id, sign_percent, "APK 签名中");
    let runtime = java_runtime::resolve(settings, Some(app_data_dir))?;
    let mut apksigner = Command::new(&runtime.java_bin);
    apksigner
        .args(build_apksigner_args(
            &build_tools.apksigner_jar.to_string_lossy(),
            &temporary_aligned.to_string_lossy(),
            &temporary_signed.to_string_lossy(),
            &signature.keystore_path,
            &signature.key_alias,
            schemes,
        ))
        .env(KEYSTORE_PASSWORD_ENV, &signature.keystore_password)
        .env(KEY_PASSWORD_ENV, &signature.key_password);
    if !run_streamed_command(app, task_id, "apksigner", apksigner).await? {
        return Ok(None);
    }
    if !temporary_signed.is_file() {
        progress::emit_error(app, task_id, "apksigner 未生成签名后的 APK");
        return Ok(None);
    }
    if schemes.v4 && !temporary_idsig.is_file() {
        progress::emit_error(app, task_id, "apksigner 未生成 V4 .idsig 文件");
        return Ok(None);
    }

    if final_apk.exists() {
        tokio::fs::remove_file(&final_apk).await?;
    }
    tokio::fs::rename(&temporary_signed, &final_apk).await?;
    if final_idsig.exists() {
        tokio::fs::remove_file(&final_idsig).await?;
    }
    let idsig_path = if schemes.v4 {
        if let Err(error) = tokio::fs::rename(&temporary_idsig, &final_idsig).await {
            let _ = tokio::fs::remove_file(&final_apk).await;
            return Err(error.into());
        }
        Some(final_idsig.to_string_lossy().into_owned())
    } else {
        None
    };

    let apk_path = final_apk.to_string_lossy().into_owned();
    progress::emit_log(app, task_id, &format!("签名 APK: {apk_path}"), "info");
    if let Some(path) = &idsig_path {
        progress::emit_log(app, task_id, &format!("V4 idsig: {path}"), "info");
    }
    progress::emit_progress(app, task_id, 100.0, "完成");
    Ok(Some(SigningOutput {
        apk_path,
        idsig_path,
    }))
}

async fn run_streamed_command(
    app: &AppHandle,
    task_id: &str,
    tool: &str,
    mut command: Command,
) -> AppResult<bool> {
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|error| AppError::Config(format!("无法启动 {tool}: {error}")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Config(format!("{tool} stdout unavailable")))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::Config(format!("{tool} stderr unavailable")))?;
    let mut out_reader = BufReader::new(stdout).lines();
    let mut err_reader = BufReader::new(stderr).lines();
    let mut exited = false;
    let mut stdout_done = false;
    let mut stderr_done = false;
    let mut success = false;
    let mut exit_code = None;

    while !(stdout_done && stderr_done && exited) {
        tokio::select! {
            line = out_reader.next_line(), if !stdout_done => match line {
                Ok(Some(line)) => progress::emit_log(app, task_id, &line, "info"),
                Ok(None) => stdout_done = true,
                Err(error) => {
                    progress::emit_log(app, task_id, &format!("{tool} stdout: {error}"), "warn");
                    stdout_done = true;
                }
            },
            line = err_reader.next_line(), if !stderr_done => match line {
                Ok(Some(line)) => {
                    let level = if line.to_lowercase().contains("error") { "error" } else { "warn" };
                    progress::emit_log(app, task_id, &line, level);
                }
                Ok(None) => stderr_done = true,
                Err(error) => {
                    progress::emit_log(app, task_id, &format!("{tool} stderr: {error}"), "warn");
                    stderr_done = true;
                }
            },
            status = child.wait(), if !exited => match status {
                Ok(status) => {
                    success = status.success();
                    exit_code = status.code();
                    exited = true;
                }
                Err(error) => return Err(error.into()),
            },
        }
    }

    if !success {
        let code = exit_code
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".into());
        progress::emit_error(app, task_id, &format!("{tool} 退出失败 (exit={code})"));
    }
    Ok(success)
}

pub async fn sign(
    app: &AppHandle,
    registry: &TaskRegistry,
    settings: &Settings,
    apk_path: &str,
    signature_id: &str,
    schemes: SigningSchemes,
) -> AppResult<TaskHandle> {
    schemes.validate()?;
    let build_tools = resolve_build_tools(settings)?;
    let signature = signature_manager::find(app, signature_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("signature {signature_id}")))?;
    let task_id = uuid::Uuid::new_v4().to_string();
    let _ = registry.register(&task_id);
    let app_clone = app.clone();
    let task_id_clone = task_id.clone();
    let apk_owned = apk_path.to_string();
    let settings_clone = settings.clone();
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;

    tokio::spawn(async move {
        match run_signing(
            &app_clone,
            &task_id_clone,
            &build_tools,
            &apk_owned,
            &signature,
            schemes,
            0.0,
            &settings_clone,
            &dir,
        )
        .await
        {
            Ok(Some(output)) => {
                progress::emit_done(&app_clone, &task_id_clone, Some(output.apk_path.as_str()));
            }
            Ok(None) => {}
            Err(error) => {
                progress::emit_error(&app_clone, &task_id_clone, &format!("签名失败: {error}"));
            }
        }
    });

    Ok(TaskHandle {
        task_id,
        kind: "sign".into(),
    })
}

pub fn format_apk_signed_path(apk: &str) -> String {
    let path = Path::new(apk);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("output");
    let parent_raw = path.parent().and_then(|value| value.to_str()).unwrap_or(".");
    let parent = if parent_raw.is_empty() {
        "."
    } else {
        parent_raw.trim_end_matches('/')
    };
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("apk");
    format!("{parent}/{stem}-aligned-signed.{extension}")
}

pub fn find_signed_apk(apk: &str) -> Option<String> {
    let path = Path::new(apk);
    let parent = path.parent()?;

    // Build the predicted path via PathBuf::join so the result uses
    // the host's separator (Windows \\ or Unix /). Comparing it via
    // `to_string_lossy()` matches what `PathBuf::join(...)` produced
    // for the same file on the other side of the test assertion.
    //
    // The previous version rebuilt the path with hard-coded "/"`s
    // which produced a mixed-separator string on Windows that did not
    // equal the PathBuf::to_str() output the test compared against.
    let predicted_path = {
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("output");
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("apk");
        let parent_path = if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        };
        parent_path.join(format!("{stem}-aligned-signed.{extension}"))
    };

    if predicted_path.exists() {
        return Some(predicted_path.to_string_lossy().into_owned());
    }

    let input_mtime = std::fs::metadata(apk).ok()?.modified().ok()?;
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(parent).ok()?.flatten() {
        let path = entry.path();
        let is_apk = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("apk"));
        if !is_apk {
            continue;
        }
        let name = match path.file_name().and_then(|value| value.to_str()) {
            Some(name) => name,
            None => continue,
        };
        if !name.contains("Signed") && !name.contains("signed") {
            continue;
        }
        let modified = match entry.metadata().ok().and_then(|metadata| metadata.modified().ok()) {
            Some(modified) => modified,
            None => continue,
        };
        // Skip only strictly older files. `<=` would exclude files
        // whose mtime is byte-identical to the input's, which happens
        // on NTFS through CI VMs where two back-to-back writes land
        // on the same 100ns mtime tick. The fallback below is then
        // empty and we return None even though the signed file is
        // right there.
        if modified < input_mtime {
            continue;
        }
        if best
            .as_ref()
            .map_or(true, |(current, _)| modified > *current)
        {
            best = Some((modified, path));
        }
    }
    best.map(|(_, path)| path.to_string_lossy().into_owned())
}

#[allow(clippy::too_many_arguments)]
pub async fn run_rotation_signing(
    app: &AppHandle,
    task_id: &str,
    build_tools: &BuildToolsPaths,
    apk: &str,
    old_signature: &SignatureConfig,
    new_signature: &SignatureConfig,
    lineage_path: &str,
    v4_enabled: bool,
    progress_base: f32,
    settings: &Settings,
    app_data_dir: &Path,
) -> AppResult<Option<SigningOutput>> {
    if old_signature.id == new_signature.id {
        return Err(AppError::InvalidInput(
            "密钥轮换的旧/新签名不能相同".into(),
        ));
    }
    let lineage = Path::new(lineage_path);
    if !lineage.is_file() {
        return Err(AppError::InvalidInput(format!(
            "lineage 文件不存在: {lineage_path}"
        )));
    }
    let input = PathBuf::from(apk);
    if !input.is_file() {
        return Err(AppError::InvalidInput(format!("APK 文件不存在: {apk}")));
    }
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("output");
    let temporary_aligned = parent.join(format!(".{stem}-{task_id}-aligned.apk"));
    let temporary_signed = parent.join(format!(".{stem}-{task_id}-signed.apk"));
    let temporary_idsig = idsig_path(&temporary_signed);
    let final_apk = PathBuf::from(format_apk_signed_path(apk));
    let final_idsig = idsig_path(&final_apk);
    let _cleanup = scopeguard::guard(
        vec![
            temporary_aligned.clone(),
            temporary_signed.clone(),
            temporary_idsig.clone(),
        ],
        |paths| {
            for path in paths {
                let _ = std::fs::remove_file(path);
            }
        },
    );
    let _ = tokio::fs::remove_file(&temporary_aligned).await;
    let _ = tokio::fs::remove_file(&temporary_signed).await;
    let _ = tokio::fs::remove_file(&temporary_idsig).await;

    progress::emit_progress(app, task_id, progress_base, "APK 对齐中");
    let mut zipalign = Command::new(&build_tools.zipalign);
    zipalign.args(build_zipalign_args(
        &input.to_string_lossy(),
        &temporary_aligned.to_string_lossy(),
    ));
    if !run_streamed_command(app, task_id, "zipalign", zipalign).await? {
        return Ok(None);
    }
    if !temporary_aligned.is_file() {
        progress::emit_error(app, task_id, "zipalign 未生成对齐后的 APK");
        return Ok(None);
    }

    let sign_percent = progress_base + (100.0 - progress_base) * 0.45;
    progress::emit_progress(app, task_id, sign_percent, "APK 密钥轮换签名中");
    let runtime = java_runtime::resolve(settings, Some(app_data_dir))?;
    let mut apksigner = Command::new(&runtime.java_bin);
    apksigner
        .args(build_apksigner_rotation_args(
            &build_tools.apksigner_jar.to_string_lossy(),
            &temporary_aligned.to_string_lossy(),
            &temporary_signed.to_string_lossy(),
            &old_signature.keystore_path,
            &old_signature.key_alias,
            &new_signature.keystore_path,
            &new_signature.key_alias,
            lineage_path,
            v4_enabled,
        ))
        .env(KEYSTORE_PASSWORD_ENV_OLD, &old_signature.keystore_password)
        .env(KEY_PASSWORD_ENV_OLD, &old_signature.key_password)
        .env(KEYSTORE_PASSWORD_ENV, &new_signature.keystore_password)
        .env(KEY_PASSWORD_ENV, &new_signature.key_password);
    if !run_streamed_command(app, task_id, "apksigner", apksigner).await? {
        return Ok(None);
    }
    if !temporary_signed.is_file() {
        progress::emit_error(app, task_id, "apksigner 未生成签名后的 APK");
        return Ok(None);
    }
    if v4_enabled && !temporary_idsig.is_file() {
        progress::emit_error(app, task_id, "apksigner 未生成 V4 .idsig 文件");
        return Ok(None);
    }

    if final_apk.exists() {
        tokio::fs::remove_file(&final_apk).await?;
    }
    tokio::fs::rename(&temporary_signed, &final_apk).await?;
    if final_idsig.exists() {
        tokio::fs::remove_file(&final_idsig).await?;
    }
    let idsig_path_value = if v4_enabled {
        if let Err(error) = tokio::fs::rename(&temporary_idsig, &final_idsig).await {
            let _ = tokio::fs::remove_file(&final_apk).await;
            return Err(error.into());
        }
        Some(final_idsig.to_string_lossy().into_owned())
    } else {
        None
    };

    let apk_path = final_apk.to_string_lossy().into_owned();
    progress::emit_log(app, task_id, &format!("签名 APK: {apk_path}"), "info");
    if let Some(path) = &idsig_path_value {
        progress::emit_log(app, task_id, &format!("V4 idsig: {path}"), "info");
    }
    progress::emit_progress(app, task_id, 100.0, "完成");
    Ok(Some(SigningOutput {
        apk_path,
        idsig_path: idsig_path_value,
    }))
}

pub async fn sign_rotation(
    app: &AppHandle,
    registry: &TaskRegistry,
    settings: &Settings,
    apk_path: &str,
    lineage_id: &str,
    v4_enabled: bool,
) -> AppResult<TaskHandle> {
    let build_tools = resolve_build_tools(settings)?;
    let lineage = lineage_manager::find(app, lineage_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("lineage {lineage_id}")))?;
    let old_signature = signature_manager::find(app, &lineage.old_signature_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("signature {}", lineage.old_signature_id)))?;
    let new_signature = signature_manager::find(app, &lineage.new_signature_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("signature {}", lineage.new_signature_id)))?;
    if old_signature.id == new_signature.id {
        return Err(AppError::InvalidInput(
            "lineage 引用的旧/新签名相同".into(),
        ));
    }
    let settings_clone = settings.clone();
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let lineage_path = PathBuf::from(&lineage.lineage_path);
    if !lineage_path.is_file() {
        return Err(AppError::InvalidInput(format!(
            "lineage 文件已丢失: {}",
            lineage.lineage_path
        )));
    }
    let task_id = uuid::Uuid::new_v4().to_string();
    let _ = registry.register(&task_id);
    let app_clone = app.clone();
    let task_id_clone = task_id.clone();
    let apk_owned = apk_path.to_string();
    let lineage_owned = lineage.lineage_path.clone();

    tokio::spawn(async move {
        match run_rotation_signing(
            &app_clone,
            &task_id_clone,
            &build_tools,
            &apk_owned,
            &old_signature,
            &new_signature,
            &lineage_owned,
            v4_enabled,
            0.0,
            &settings_clone,
            &dir,
        )
        .await
        {
            Ok(Some(output)) => {
                progress::emit_done(&app_clone, &task_id_clone, Some(output.apk_path.as_str()));
            }
            Ok(None) => {}
            Err(error) => {
                progress::emit_error(&app_clone, &task_id_clone, &format!("签名失败: {error}"));
            }
        }
    });

    Ok(TaskHandle {
        task_id,
        kind: "sign".into(),
    })
}

fn idsig_path(apk: &Path) -> PathBuf {
    let mut value = apk.as_os_str().to_os_string();
    value.push(".idsig");
    PathBuf::from(value)
}

/// Build the CLI argument vector for `uber-apk-signer --ks`. The JAR is
/// invoked by `java -jar`; this function returns only the tail of args
/// (the `java` half is composed by the caller).
///
/// Mirrors `build_apksigner_rotation_args` but for the simpler
/// "single keystore, one signing round" case used when the user uploads
/// a brand-new APK + keystore in Settings → Sign.
///
/// Note: passwords are passed as **plain CLI strings** rather than as
/// `env:` references because uber-apk-signer does not understand `env:`
/// the way apksigner does. Callers should ensure the command-line is
/// not visible to other processes during signing.
pub fn build_uber_args(
    jar: &str,
    apk: &str,
    keystore: &str,
    store_password: &str,
    alias: &str,
    key_password: &str,
) -> Vec<String> {
    vec![
        "-jar".into(),
        jar.into(),
        "--apks".into(),
        apk.into(),
        "--ks".into(),
        keystore.into(),
        "--ksPass".into(),
        store_password.into(),
        "--ksAlias".into(),
        alias.into(),
        "--ksKeyPass".into(),
        key_password.into(),
    ]
}
