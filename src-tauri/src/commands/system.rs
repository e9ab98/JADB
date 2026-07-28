use crate::error::{AppError, AppResult};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tauri::{AppHandle, Manager};

/// Pick the most-recently-modified file inside `dir`. Returns `dir` itself when
/// the directory is empty so callers can still surface a path.
pub fn latest_file_in(dir: &Path) -> AppResult<PathBuf> {
    let entries = std::fs::read_dir(dir).map_err(AppError::Io)?;
    let mut files: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    if files.is_empty() {
        return Ok(dir.to_path_buf());
    }
    files.sort_by_key(|e| {
        e.metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .unwrap_or(SystemTime::UNIX_EPOCH)
    });
    Ok(files.last().expect("non-empty checked above").path())
}

#[tauri::command]
pub async fn get_log_path(app: AppHandle) -> AppResult<String> {
    let dir = app
        .path()
        .app_log_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    if !dir.exists() {
        return Err(AppError::NotFound(format!(
            "log directory does not exist yet: {}",
            dir.display()
        )));
    }
    Ok(latest_file_in(&dir)?.to_string_lossy().to_string())
}
