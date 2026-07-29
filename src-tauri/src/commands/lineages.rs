use crate::commands::signatures::SignatureConfig;
use crate::error::{AppError, AppResult};
use crate::progress;
use crate::services::apk_signer::resolve_build_tools;
use crate::services::java_runtime;
use crate::services::lineage_manager::{self, LineageConfig};
use crate::services::signature_manager;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tokio::process::Command;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LineageStatus {
    pub config: LineageConfig,
    pub file_exists: bool,
    pub old_signature_exists: bool,
    pub new_signature_exists: bool,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CreateLineageInput {
    pub label: String,
    pub old_signature_id: String,
    pub new_signature_id: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ImportLineageInput {
    pub label: String,
    pub src_path: String,
    pub old_signature_id: String,
    pub new_signature_id: String,
}

/// apksigner rotate first appeared in Build-Tools 30.0.0; V3.1 lineage support
/// is documented in Build-Tools 31.0.0. Require >= 31.0.0 to keep things safe.
fn ensure_rotation_supported(version: &str) -> AppResult<()> {
    let parts: Vec<u32> = version
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse::<u32>().ok())
        .collect();
    if parts.len() < 2 {
        return Err(AppError::ToolFailed {
            tool: "apksigner".into(),
            code: -1,
            msg: format!("无法解析 Build-Tools 版本: {version}"),
        });
    }
    let (major, minor) = (parts[0], parts[1]);
    if (major, minor) < (31, 0) {
        return Err(AppError::ToolFailed {
            tool: "apksigner".into(),
            code: -1,
            msg: format!(
                "当前 Build-Tools 版本 ({version}) 不支持密钥轮换,请升级到 31.0.0 及以上"
            ),
        });
    }
    Ok(())
}

fn build_rotate_args(
    apksigner_jar: &str,
    old_keystore: &str,
    old_alias: &str,
    new_keystore: &str,
    new_alias: &str,
    output_lineage: &str,
) -> Vec<String> {
    // The `apksigner rotate` CLI requires two signer groups, each introduced
    // by `--old-signer` / `--new-signer`, and each carrying its own
    // `--ks / --ks-key-alias / --ks-pass / --key-pass`. The previous
    // implementation passed the keystore path as `--in`, which actually
    // expects an *existing* SigningCertificateLineage blob (or a signed
    // APK). Confusing the two surfaced as
    // `Cannot invoke SignerParams.isEmpty() because oldSignerParams is null`
    // from ApkSignerTool.rotate. Verified against the in-jar help_rotate.txt
    // shipped with Build-Tools 36.0.0.
    //
    // The keystores passed here are pre-converted to passwordless PKCS12
    // files by `write_unprotected_p12`, so we use `pass:` (empty password)
    // for both password flags on both signer groups.
    let signer = |keystore: &str, alias: &str| -> [String; 6] {
        [
            "--ks".into(),
            keystore.into(),
            "--ks-key-alias".into(),
            alias.into(),
            "--ks-pass".into(),
            "pass:".into(),
        ]
    };
    let old = signer(old_keystore, old_alias);
    let new_ = signer(new_keystore, new_alias);
    vec![
        "-Xmx1024M".into(),
        "-jar".into(),
        apksigner_jar.into(),
        "rotate".into(),
        "--out".into(),
        output_lineage.into(),
        "--old-signer".into(),
        "--ks-pass".into(),
        "pass:".into(),
        "--key-pass".into(),
        "pass:".into(),
        old[0].clone(),
        old[1].clone(),
        old[2].clone(),
        old[3].clone(),
        old[4].clone(),
        old[5].clone(),
        "--new-signer".into(),
        "--ks-pass".into(),
        "pass:".into(),
        "--key-pass".into(),
        "pass:".into(),
        new_[0].clone(),
        new_[1].clone(),
        new_[2].clone(),
        new_[3].clone(),
        new_[4].clone(),
        new_[5].clone(),
    ]
}

async fn resolve_signatures(
    app: &AppHandle,
    old_id: &str,
    new_id: &str,
) -> AppResult<(SignatureConfig, SignatureConfig)> {
    if old_id.trim().is_empty() || new_id.trim().is_empty() {
        return Err(AppError::InvalidInput("签名引用不能为空".into()));
    }
    if old_id == new_id {
        return Err(AppError::InvalidInput("旧签名与新签名不能相同".into()));
    }
    let old = signature_manager::find(app, old_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("signature {old_id}")))?;
    let new = signature_manager::find(app, new_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("signature {new_id}")))?;
    Ok((old, new))
}

/// Convert a password-protected keystore (JKS / PKCS12) into a
/// passwordless PKCS12 copy using a tiny Java helper launched via
/// `java <source-file>`. The output file is suitable for `apksigner
/// rotate --ks ...` with `--ks-pass pass:` / `--key-pass pass:`.
async fn write_unprotected_p12(
    src: &Path,
    src_store_pwd: &str,
    key_pwd: &str,
    alias: &str,
    dest: &Path,
    helper: &Path,
    java_bin: &Path,
) -> AppResult<()> {
    const HELPER_SOURCE: &str = r#"
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.security.Key;
import java.security.KeyStore;
import java.security.cert.Certificate;

public class JadbMakeUnprotectedP12 {
    public static void main(String[] args) throws Exception {
        if (args.length != 5) {
            System.err.println("Usage: java JadbMakeUnprotectedP12 <srcJks> <srcPwd> <keyPwd> <alias> <destP12>");
            System.exit(2);
        }
        String srcPath = args[0];
        char[] srcPwd = args[1].toCharArray();
        char[] keyPwd = args[2].toCharArray();
        String alias = args[3];
        String destPath = args[4];

        KeyStore src = KeyStore.getInstance("JKS");
        try (FileInputStream fis = new FileInputStream(srcPath)) {
            src.load(fis, srcPwd);
        }
        if (!src.containsAlias(alias)) {
            System.err.println("Alias not found in source keystore: " + alias);
            System.exit(3);
        }
        Key key = src.getKey(alias, keyPwd);
        Certificate[] chain = src.getCertificateChain(alias);
        if (chain == null) {
            chain = new Certificate[] { src.getCertificate(alias) };
        }

        KeyStore dest = KeyStore.getInstance("PKCS12");
        dest.load(null, null);
        dest.setKeyEntry(alias, key, new char[0], chain);

        try (FileOutputStream fos = new FileOutputStream(destPath)) {
            dest.store(fos, new char[0]);
        }
        System.out.println("OK");
    }
}
"#;
    if let Err(error) = tokio::fs::write(helper, HELPER_SOURCE.as_bytes()).await {
        return Err(AppError::Config(format!("写入 Java helper 失败: {error}")));
    }
    let src_str = src.to_string_lossy().into_owned();
    let dest_str = dest.to_string_lossy().into_owned();
    let helper_str = helper.to_string_lossy().into_owned();
    let output = Command::new(java_bin)
        // Force PKCS12 to use the legacy PBE-SHA1-RC2-40 algorithm so the
        // resulting keystore is readable by older apksigner.rotate builds
        // that do not understand PBES2 / SHA-256 / AES.
        .arg("-Dkeystore.pkcs12.legacy=true")
        .arg(&helper_str)
        .args([src_str.as_str(), src_store_pwd, key_pwd, alias, dest_str.as_str()])
        .output()
        .await
        .map_err(|error| {
            AppError::Config(format!("启动 java helper 失败: {error}"))
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        return Err(AppError::ToolFailed {
            tool: "java".into(),
            code: output.status.code().unwrap_or(-1),
            msg: format!(
                "Java helper 转换 keystore 失败(请确认 alias 与密码, {src_str}): {stderr}{stdout}"
            ),
        });
    }
    Ok(())
}

#[tauri::command]
pub async fn create_lineage(
    app: AppHandle,
    input: CreateLineageInput,
) -> AppResult<LineageConfig> {
    if input.label.trim().is_empty() {
        return Err(AppError::InvalidInput("Lineage 名称不能为空".into()));
    }
    let (old, new) = resolve_signatures(&app, &input.old_signature_id, &input.new_signature_id).await?;
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let settings = crate::config::settings::read(&dir).await?;
    let build_tools = resolve_build_tools(&settings)?;
    ensure_rotation_supported(&build_tools.version)?;

    let ts = chrono::Utc::now().timestamp_millis();
    let pid = std::process::id();
    let tmp = std::env::temp_dir().join(format!(".jadb-rotate-{pid}-{ts}.lineage"));
    let old_p12 = std::env::temp_dir().join(format!(".jadb-rotate-{pid}-{ts}-old.p12"));
    let new_p12 = std::env::temp_dir().join(format!(".jadb-rotate-{pid}-{ts}-new.p12"));
    let java_helper = std::env::temp_dir()
        .join(format!(".jadb-rotate-{pid}-{ts}-MakeUnprotectedP12.java"));

    // The old signer is needed as both `--ks` inside `--old-signer --ks ...`
    // and (if a future --in lineage is supplied) to verify the leaf
    // descendant. We re-derive the passwordless PKCS12 copy from the
    // user's stored keystore every time so we never write a password to
    // disk.
    progress::emit_progress(
        &app,
        "lineage-rotate",
        0.1,
        "正在转换旧 keystore 为无密码 PKCS12",
    );
    let runtime = java_runtime::resolve(&settings, Some(&dir))?;
    if let Err(error) = write_unprotected_p12(
        Path::new(&old.keystore_path),
        &old.keystore_password,
        &old.key_password,
        &old.key_alias,
        &old_p12,
        &java_helper,
        &runtime.java_bin,
    )
    .await
    {
        let _ = tokio::fs::remove_file(&old_p12).await;
        let _ = tokio::fs::remove_file(&new_p12).await;
        let _ = tokio::fs::remove_file(&java_helper).await;
        return Err(error);
    }
    let _ = tokio::fs::remove_file(&java_helper).await;
    if !old_p12.is_file() {
        let _ = tokio::fs::remove_file(&new_p12).await;
        return Err(AppError::ToolFailed {
            tool: "java".into(),
            code: -1,
            msg: "Java helper 未生成旧签名无密码 PKCS12".into(),
        });
    }

    progress::emit_progress(
        &app,
        "lineage-rotate",
        0.3,
        "正在转换新 keystore 为无密码 PKCS12",
    );
    if let Err(error) = write_unprotected_p12(
        Path::new(&new.keystore_path),
        &new.keystore_password,
        &new.key_password,
        &new.key_alias,
        &new_p12,
        &java_helper,
        &runtime.java_bin,
    )
    .await
    {
        let _ = tokio::fs::remove_file(&old_p12).await;
        let _ = tokio::fs::remove_file(&new_p12).await;
        let _ = tokio::fs::remove_file(&java_helper).await;
        return Err(error);
    }
    let _ = tokio::fs::remove_file(&java_helper).await;
    if !new_p12.is_file() {
        let _ = tokio::fs::remove_file(&old_p12).await;
        return Err(AppError::ToolFailed {
            tool: "java".into(),
            code: -1,
            msg: "Java helper 未生成新签名无密码 PKCS12".into(),
        });
    }

    let apksigner_jar = build_tools.apksigner_jar.to_string_lossy().into_owned();
    let old_p12_str = old_p12.to_string_lossy().into_owned();
    let new_p12_str = new_p12.to_string_lossy().into_owned();
    let tmp_str = tmp.to_string_lossy().into_owned();

    progress::emit_progress(&app, "lineage-rotate", 0.5, "正在生成 lineage 文件");
    let output = Command::new(&runtime.java_bin)
        .args(build_rotate_args(
            &apksigner_jar,
            &old_p12_str,
            &old.key_alias,
            &new_p12_str,
            &new.key_alias,
            &tmp_str,
        ))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| AppError::Config(format!("启动 apksigner rotate 失败: {e}")))?;
    let rotate_succeeded = output.status.success();
    let rotate_stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let rotate_stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let _ = tokio::fs::remove_file(&old_p12).await;
    let _ = tokio::fs::remove_file(&new_p12).await;
    if !rotate_succeeded {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(AppError::ToolFailed {
            tool: "apksigner".into(),
            code: output.status.code().unwrap_or(-1),
            msg: format!(
                "apksigner rotate 失败: {rotate_stderr}
{rotate_stdout}"
            ),
        });
    }
    if !tmp.is_file() {
        return Err(AppError::ToolFailed {
            tool: "apksigner".into(),
            code: output.status.code().unwrap_or(-1),
            msg: format!(
                "apksigner rotate 未生成 lineage 文件: {rotate_stderr}"
            ),
        });
    }

    let saved = lineage_manager::persist(
        &app,
        input.label,
        &tmp,
        old.id,
        new.id,
        false,
    )
    .await;
    let _ = tokio::fs::remove_file(&tmp).await;
    saved
}

#[tauri::command]
pub async fn list_lineages(app: AppHandle) -> AppResult<Vec<LineageStatus>> {
    let lineages = lineage_manager::read_all(&app).await?;
    let signatures = signature_manager::read_all(&app).await?;
    let mut out = Vec::with_capacity(lineages.len());
    for lineage in lineages {
        let file_exists = PathBuf::from(&lineage.lineage_path).is_file();
        let old_signature_exists = signatures.iter().any(|s| s.id == lineage.old_signature_id);
        let new_signature_exists = signatures.iter().any(|s| s.id == lineage.new_signature_id);
        out.push(LineageStatus {
            config: lineage,
            file_exists,
            old_signature_exists,
            new_signature_exists,
        });
    }
    Ok(out)
}

#[tauri::command]
pub async fn import_lineage(
    app: AppHandle,
    input: ImportLineageInput,
) -> AppResult<LineageConfig> {
    if input.label.trim().is_empty() {
        return Err(AppError::InvalidInput("Lineage 名称不能为空".into()));
    }
    let src = PathBuf::from(&input.src_path);
    if !src.is_file() {
        return Err(AppError::InvalidInput(format!(
            "lineage 源文件不存在: {}",
            src.display()
        )));
    }
    let (old, new) = resolve_signatures(&app, &input.old_signature_id, &input.new_signature_id).await?;
    lineage_manager::persist(
        &app,
        input.label,
        &src,
        old.id,
        new.id,
        true,
    )
    .await
}

#[tauri::command]
pub async fn export_lineage(
    app: AppHandle,
    id: String,
    dest_path: String,
) -> AppResult<String> {
    let dest = PathBuf::from(&dest_path);
    lineage_manager::export_lineage_file(&app, &id, &dest).await
}

#[tauri::command]
pub async fn delete_lineage(app: AppHandle, id: String) -> AppResult<()> {
    lineage_manager::remove(&app, &id).await
}
