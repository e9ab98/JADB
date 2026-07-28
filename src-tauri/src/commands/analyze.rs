use crate::config::settings;
use crate::error::{AppError, AppResult};
use crate::services::apk_analyzer::{self, ApkInfo};
use crate::services::task_registry::TaskRegistry;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

#[tauri::command]
pub async fn analyze_apk(app: AppHandle, path: String) -> AppResult<ApkInfo> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    let apk_path = PathBuf::from(&path);
    if !apk_path.exists() {
        return Err(AppError::InvalidInput(format!("file not found: {path}")));
    }
    apk_analyzer::analyze(&s, &apk_path).await
}

#[tauri::command]
pub async fn cancel_task(
    registry: State<'_, TaskRegistry>,
    task_id: String,
) -> AppResult<()> {
    if registry.cancel(&task_id) {
        Ok(())
    } else {
        Err(AppError::TaskNotFound(task_id))
    }
}

#[tauri::command]
pub async fn file_size(path: String) -> AppResult<u64> {
    let p = std::path::PathBuf::from(&path);
    if !p.exists() {
        return Err(AppError::InvalidInput(format!("file not found: {path}")));
    }
    let meta = std::fs::metadata(&p).map_err(AppError::Io)?;
    Ok(meta.len())
}
