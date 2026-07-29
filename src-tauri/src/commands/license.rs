use crate::error::AppResult;
use crate::services::license::{LicenseService, LicenseStatus};
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn get_device_id(app: AppHandle, license: State<'_, LicenseService>) -> AppResult<String> {
    license.device_id(&app).await
}

#[tauri::command]
pub async fn get_license_status(app: AppHandle, license: State<'_, LicenseService>) -> AppResult<LicenseStatus> {
    license.status(&app).await
}

#[tauri::command]
pub async fn activate_license(app: AppHandle, license: State<'_, LicenseService>, token: String) -> AppResult<LicenseStatus> {
    license.activate(&app, &token).await
}

#[tauri::command]
pub async fn remove_license(app: AppHandle, license: State<'_, LicenseService>) -> AppResult<LicenseStatus> {
    license.remove(&app).await
}
