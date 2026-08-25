//! Tauri commands backing the Fastboot tab inside DeviceView.
//!
//! Each command reads `settings.json` fresh (no shared state with the
//! adb commands) so the user's adb path config is the single source of
//! truth for both adb and fastboot. The fastboot binary is resolved
//! from `adb_path`'s parent directory inside
//! `fastboot_manager::fastboot_binary`; if it isn't there, every
//! command fails with `tool missing: fastboot` and the UI shows the
//! "install Platform-Tools" banner.

use crate::config::settings;
use crate::error::{AppError, AppResult};
use crate::services::fastboot_manager::{self, FastbootDevice, FastbootVarInfo, OemDeviceInfo};
use tauri::{AppHandle, Manager};

#[tauri::command]
pub async fn fastboot_devices(app: AppHandle) -> AppResult<Vec<FastbootDevice>> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    fastboot_manager::list_devices(&s).await
}

#[tauri::command]
pub async fn fastboot_reboot(
    app: AppHandle,
    device: String,
    mode: Option<String>,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    fastboot_manager::reboot(&s, &device, mode.as_deref()).await
}

#[tauri::command]
pub async fn fastboot_get_info(
    app: AppHandle,
    device: String,
) -> AppResult<FastbootVarInfo> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    fastboot_manager::get_info(&s, &device).await
}

#[tauri::command]
pub async fn fastboot_oem_device_info(
    app: AppHandle,
    device: String,
) -> AppResult<OemDeviceInfo> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    fastboot_manager::oem_device_info(&s, &device).await
}
