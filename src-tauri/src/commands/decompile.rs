use crate::config::settings;
use crate::error::{AppError, AppResult};
use crate::services::apk_decompiler::{self, TaskHandle};
use crate::services::task_registry::TaskRegistry;
use std::process::Stdio;
use tauri::{AppHandle, Manager, State};
use tokio::process::Command;

#[tauri::command]
pub async fn decompile_apk(
    app: AppHandle,
    registry: State<'_, TaskRegistry>,
    path: String,
    out_dir: String,
    force: bool,
) -> AppResult<TaskHandle> {
    if path.trim().is_empty() {
        return Err(AppError::InvalidInput("path is empty".into()));
    }
    if out_dir.trim().is_empty() {
        return Err(AppError::InvalidInput("out_dir is empty".into()));
    }
    if !std::path::Path::new(&path).exists() {
        return Err(AppError::InvalidInput(format!("file not found: {path}")));
    }
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let settings = settings::read(&dir).await?;
    apk_decompiler::decompile(&app, &registry, &settings, &path, &out_dir, force).await
}

#[tauri::command]
pub async fn open_path(path: String) -> AppResult<()> {
    if path.trim().is_empty() {
        return Err(AppError::InvalidInput("path is empty".into()));
    }
    // Spawn the OS handler. `open` on macOS, `xdg-open` on Linux, `cmd /c start ""` on Windows.
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = Command::new("open");
        c.arg(&path);
        c
    };
    #[cfg(target_os = "linux")]
    let mut cmd = {
        let mut c = Command::new("xdg-open");
        c.arg(&path);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = Command::new("cmd");
        c.arg("/c").arg("start").arg("").arg(&path);
        c
    };
    cmd.stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| AppError::Config(format!("open path failed: {e}")))?;
    Ok(())
}
