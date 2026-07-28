use crate::config::settings::{read, write, Settings, SettingsPatch};
use crate::error::{AppError, AppResult};
use tauri::{AppHandle, Emitter, Manager};

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> AppResult<Settings> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    read(&dir).await
}

#[tauri::command]
pub async fn update_settings(app: AppHandle, patch: SettingsPatch) -> AppResult<Settings> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let mut s = read(&dir).await?;
    s.apply(patch);
    write(&dir, &s).await?;
    let _ = app.emit("settings://changed", &s);
    Ok(s)
}
