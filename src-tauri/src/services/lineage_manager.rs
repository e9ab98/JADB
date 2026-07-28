use crate::commands::signatures::SignatureConfig;
use crate::error::{AppError, AppResult};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tokio::fs;

pub const FILE_NAME: &str = "lineages.json";
pub const LINEAGE_SUBDIR: &str = "lineages";

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LineageConfig {
    pub id: String,
    pub label: String,
    pub lineage_path: String,
    pub old_signature_id: String,
    pub new_signature_id: String,
    pub created_at: String,
}

pub fn base_dir(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    Ok(dir.join(LINEAGE_SUBDIR))
}

pub fn config_path(app: &AppHandle) -> AppResult<PathBuf> {
    Ok(base_dir(app)?.join(FILE_NAME))
}

pub fn lineage_file_path(app: &AppHandle, id: &str) -> AppResult<PathBuf> {
    Ok(base_dir(app)?.join(format!("{id}.lineage")))
}

async fn write_json(path: &Path, bytes: &[u8]) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::write(path, bytes).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(path)?.permissions();
        perm.set_mode(0o600);
        std::fs::set_permissions(path, perm)?;
    }
    Ok(())
}

pub async fn read_all(app: &AppHandle) -> AppResult<Vec<LineageConfig>> {
    let p = config_path(app)?;
    if !p.exists() {
        return Ok(vec![]);
    }
    let bytes = fs::read(&p).await?;
    if bytes.is_empty() {
        return Ok(vec![]);
    }
    let list: Vec<LineageConfig> =
        serde_json::from_slice(&bytes).map_err(|e| AppError::Config(e.to_string()))?;
    Ok(list)
}

async fn write_all(app: &AppHandle, list: &[LineageConfig]) -> AppResult<()> {
    let p = config_path(app)?;
    let json = serde_json::to_vec_pretty(list).map_err(|e| AppError::Config(e.to_string()))?;
    write_json(&p, &json).await
}

pub async fn find(app: &AppHandle, id: &str) -> AppResult<Option<LineageConfig>> {
    Ok(read_all(app).await?.into_iter().find(|s| s.id == id))
}

pub fn generate_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn validate(
    old: &SignatureConfig,
    new: &SignatureConfig,
    lineage_path: &Path,
) -> AppResult<()> {
    if old.id == new.id {
        return Err(AppError::InvalidInput("旧签名与新签名不能相同".into()));
    }
    if !lineage_path.is_file() {
        return Err(AppError::InvalidInput(format!(
            "lineage 文件不存在: {}",
            lineage_path.display()
        )));
    }
    Ok(())
}

pub async fn persist(
    app: &AppHandle,
    label: String,
    lineage_src: &Path,
    old_signature_id: String,
    new_signature_id: String,
    copy: bool,
) -> AppResult<LineageConfig> {
    if label.trim().is_empty() {
        return Err(AppError::InvalidInput("Lineage 名称不能为空".into()));
    }
    if old_signature_id.trim().is_empty() || new_signature_id.trim().is_empty() {
        return Err(AppError::InvalidInput("签名引用不能为空".into()));
    }
    if old_signature_id == new_signature_id {
        return Err(AppError::InvalidInput("旧签名与新签名不能相同".into()));
    }
    if !lineage_src.is_file() {
        return Err(AppError::InvalidInput(format!(
            "lineage 源文件不存在: {}",
            lineage_src.display()
        )));
    }
    let id = generate_id();
    let dest = lineage_file_path(app, &id)?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).await?;
    }
    if copy {
        fs::copy(lineage_src, &dest).await?;
    } else {
        fs::rename(lineage_src, &dest).await?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&dest)?.permissions();
        perm.set_mode(0o600);
        std::fs::set_permissions(&dest, perm)?;
    }
    let cfg = LineageConfig {
        id,
        label,
        lineage_path: dest.to_string_lossy().into_owned(),
        old_signature_id,
        new_signature_id,
        created_at: Utc::now().to_rfc3339(),
    };
    let mut list = read_all(app).await?;
    list.push(cfg.clone());
    write_all(app, &list).await?;
    Ok(cfg)
}

pub async fn remove(app: &AppHandle, id: &str) -> AppResult<()> {
    let mut list = read_all(app).await?;
    let pos = list
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| AppError::NotFound(id.to_string()))?;
    let removed = list.remove(pos);
    write_all(app, &list).await?;
    let p = PathBuf::from(&removed.lineage_path);
    if p.exists() {
        let _ = fs::remove_file(&p).await;
    }
    Ok(())
}

/// Copy the lineage file backing `id` to `dest`. Returns the path that
/// was written. The lineage file is the binary proof-of-rotation blob
/// produced by `apksigner rotate`; copying it is a plain byte-for-byte
/// operation — re-encoding or re-signing is not needed.
pub async fn export_lineage_file(
    app: &AppHandle,
    id: &str,
    dest: &Path,
) -> AppResult<String> {
    let cfg = find(app, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("lineage {id}")))?;
    let src = PathBuf::from(&cfg.lineage_path);
    if !src.is_file() {
        return Err(AppError::InvalidInput(format!(
            "lineage 文件不存在: {}",
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

pub async fn list_referencing_signature(
    app: &AppHandle,
    signature_id: &str,
) -> AppResult<Vec<LineageConfig>> {
    Ok(read_all(app)
        .await?
        .into_iter()
        .filter(|l| l.old_signature_id == signature_id || l.new_signature_id == signature_id)
        .collect())
}
