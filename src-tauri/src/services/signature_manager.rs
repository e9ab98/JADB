use crate::commands::signatures::SignatureConfig;
use crate::error::{AppError, AppResult};
use chrono::Utc;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tauri::{AppHandle, Manager};
use tokio::fs;

const FILE_NAME: &str = "signatures.json";
const KEYSTORE_DIR: &str = "keystores";

pub fn file_path(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    Ok(dir.join(FILE_NAME))
}

pub fn keystores_dir(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    Ok(dir.join(KEYSTORE_DIR))
}

pub async fn read_all(app: &AppHandle) -> AppResult<Vec<SignatureConfig>> {
    let p = file_path(app)?;
    if !p.exists() {
        return Ok(vec![]);
    }
    let bytes = fs::read(&p).await?;
    let list: Vec<SignatureConfig> =
        serde_json::from_slice(&bytes).map_err(|e| AppError::Config(e.to_string()))?;
    Ok(list)
}

async fn write_all(app: &AppHandle, list: &[SignatureConfig]) -> AppResult<()> {
    let p = file_path(app)?;
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).await?;
    }
    let json = serde_json::to_vec_pretty(list).map_err(|e| AppError::Config(e.to_string()))?;
    fs::write(&p, &json).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&p)?.permissions();
        perm.set_mode(0o600);
        std::fs::set_permissions(&p, perm)?;
    }
    Ok(())
}

pub async fn find(app: &AppHandle, id: &str) -> AppResult<Option<SignatureConfig>> {
    Ok(read_all(app).await?.into_iter().find(|s| s.id == id))
}

pub fn generate_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub async fn create(app: &AppHandle, mut input: SignatureConfig) -> AppResult<SignatureConfig> {
    if input.id.is_empty() {
        input.id = generate_id();
    }
    if input.created_at.is_empty() {
        input.created_at = Utc::now().to_rfc3339();
    }
    let mut list = read_all(app).await?;
    list.push(input.clone());
    write_all(app, &list).await?;
    Ok(input)
}

pub async fn update(
    app: &AppHandle,
    id: &str,
    patch: SignatureConfig,
) -> AppResult<SignatureConfig> {
    let mut list = read_all(app).await?;
    let pos = list
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| AppError::NotFound(id.to_string()))?;
    let mut updated = list[pos].clone();
    apply_patch(&mut updated, &patch);
    list[pos] = updated.clone();
    write_all(app, &list).await?;
    Ok(updated)
}

/// Apply non-empty fields from `patch` onto `target`. Empty strings are treated
/// as "no change" so callers can send a partial config without clearing fields.
pub fn apply_patch(target: &mut SignatureConfig, patch: &SignatureConfig) {
    if !patch.label.is_empty() {
        target.label = patch.label.clone();
    }
    if !patch.keystore_path.is_empty() {
        target.keystore_path = patch.keystore_path.clone();
    }
    if !patch.keystore_password.is_empty() {
        target.keystore_password = patch.keystore_password.clone();
    }
    if !patch.key_alias.is_empty() {
        target.key_alias = patch.key_alias.clone();
    }
    if !patch.key_password.is_empty() {
        target.key_password = patch.key_password.clone();
    }
}

pub async fn delete(app: &AppHandle, id: &str) -> AppResult<()> {
    let mut list = read_all(app).await?;
    if let Some(pos) = list.iter().position(|s| s.id == id) {
        let removed = list.remove(pos);
        write_all(app, &list).await?;
        // Best-effort: remove the copied keystore file if any.
        if let Ok(dir) = keystores_dir(app) {
            let ext = Path::new(&removed.keystore_path)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("jks");
            let p = dir.join(format!("{}.{}", &removed.id, ext));
            if p.exists() {
                let _ = fs::remove_file(&p).await;
            }
        }
        Ok(())
    } else {
        Err(AppError::NotFound(id.to_string()))
    }
}

pub async fn import(
    app: &AppHandle,
    src: &Path,
    alias: String,
    password: String,
    label: String,
) -> AppResult<SignatureConfig> {
    if !src.exists() {
        return Err(AppError::InvalidInput(format!(
            "keystore not found: {}",
            src.display()
        )));
    }
    let id = generate_id();
    let dir = keystores_dir(app)?;
    fs::create_dir_all(&dir).await?;
    let ext = src
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("jks");
    let dest = dir.join(format!("{id}.{ext}"));
    fs::copy(src, &dest).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&dest)?.permissions();
        perm.set_mode(0o600);
        std::fs::set_permissions(&dest, perm)?;
    }
    let cfg = SignatureConfig {
        id,
        label: if label.is_empty() {
            "Untitled".into()
        } else {
            label
        },
        keystore_path: dest.to_string_lossy().into_owned(),
        keystore_password: password.clone(),
        key_alias: alias,
        key_password: password,
        created_at: Utc::now().to_rfc3339(),
    };
    create(app, cfg).await
}

/// Generate a fresh keystore via `keytool` and store it in the managed
/// `keystores/` directory. The caller supplies the alias and passwords;
/// `keytool` does not accept passwords via env vars, so they are passed as
/// arguments and visible to the local process list for the lifetime of the
/// `keytool` invocation.
#[derive(Default, Debug)]
pub struct DNameParts {
    pub cn: String,
    pub ou: String,
    pub o: String,
    pub l: String,
    pub st: String,
    pub c: String,
}

impl DNameParts {
    fn to_keytool_arg(&self) -> String {
        // keytool needs a non-empty CN, so default it if blank.
        let cn = if self.cn.trim().is_empty() { "Android" } else { self.cn.trim() };
        let mut parts = vec![format!("CN={cn}")];
        for (key, value) in [
            ("OU", &self.ou),
            ("O", &self.o),
            ("L", &self.l),
            ("ST", &self.st),
            ("C", &self.c),
        ] {
            let v = value.trim();
            if !v.is_empty() {
                parts.push(format!("{key}={v}"));
            }
        }
        parts.join(", ")
    }
}

#[derive(Default, Debug)]
pub struct KeystoreGenOptions {
    pub key_algorithm: String, // "RSA" | "EC"
    pub key_size: u32,
    pub validity_days: u32,
    pub dname: DNameParts,
}

pub async fn create_new(
    app: &AppHandle,
    label: String,
    alias: String,
    keystore_password: String,
    key_password: String,
    options: KeystoreGenOptions,
) -> AppResult<SignatureConfig> {
    if label.trim().is_empty() {
        return Err(AppError::InvalidInput("签名名称不能为空".into()));
    }
    if alias.trim().is_empty() {
        return Err(AppError::InvalidInput("alias 不能为空".into()));
    }
    if keystore_password.is_empty() || key_password.is_empty() {
        return Err(AppError::InvalidInput("keystore / key 密码不能为空".into()));
    }
    let key_algorithm = match options.key_algorithm.to_ascii_uppercase().as_str() {
        "RSA" => "RSA".to_string(),
        "EC" | "ECDSA" => "EC".to_string(),
        other => {
            return Err(AppError::InvalidInput(format!(
                "不支持的密钥算法: {other}"
            )))
        }
    };
    let key_size = options.key_size;
    let validity_days = options.validity_days;
    let dname_arg = options.dname.to_keytool_arg();

    let id = generate_id();
    let dir = keystores_dir(app)?;
    fs::create_dir_all(&dir).await?;
    let dest = dir.join(format!("{id}.jks"));
    let dest_str = dest.to_string_lossy().into_owned();

    let output = Command::new("keytool")
        .args([
            "-genkeypair",
            "-noprompt",
            "-alias", &alias,
            "-keyalg", &key_algorithm,
            "-keysize", &key_size.to_string(),
            "-validity", &validity_days.to_string(),
            "-keystore", &dest_str,
            "-storetype", "JKS",
            "-storepass", &keystore_password,
            "-keypass", &key_password,
            "-dname", &dname_arg,
        ])
        .output()
        .await
        .map_err(|error| AppError::Config(format!("启动 keytool 失败: {error}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let _ = fs::remove_file(&dest).await;
        return Err(AppError::ToolFailed {
            tool: "keytool".into(),
            code: output.status.code().unwrap_or(-1),
            msg: format!("keytool 生成 keystore 失败: {stderr}{stdout}"),
        });
    }
    if !dest.is_file() {
        return Err(AppError::ToolFailed {
            tool: "keytool".into(),
            code: -1,
            msg: "keytool 未生成 keystore 文件".into(),
        });
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&dest)?.permissions();
        perm.set_mode(0o600);
        std::fs::set_permissions(&dest, perm)?;
    }

    let cfg = SignatureConfig {
        id,
        label,
        keystore_path: dest_str,
        keystore_password,
        key_alias: alias,
        key_password,
        created_at: Utc::now().to_rfc3339(),
    };
    create(app, cfg).await
}

/// Copy the keystore file backing `signature_id` to `dest`. The source path
/// is whatever `SignatureConfig.keystore_path` currently points at (managed
/// keystore dir for created/imported entries). Returns the path that was
/// written.
pub async fn export_keystore(
    app: &AppHandle,
    signature_id: &str,
    dest: &Path,
) -> AppResult<String> {
    let cfg = find(app, signature_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("signature {signature_id}")))?;
    let src = PathBuf::from(&cfg.keystore_path);
    if !src.is_file() {
        return Err(AppError::InvalidInput(format!(
            "keystore 文件不存在: {}",
            src.display()
        )));
    }
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).await?;
        }
    }
    fs::copy(&src, dest).await?;
    Ok(dest.to_string_lossy().into_owned())
}
