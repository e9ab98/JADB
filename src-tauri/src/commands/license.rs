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

/// 强制在线同步：调 server，失败抛错（前端用 toast 提示）。
#[tauri::command]
pub async fn refresh_license_status(app: AppHandle, license: State<'_, LicenseService>) -> AppResult<LicenseStatus> {
    license.refresh(&app).await
}

/// 仅在线校验一个 token（不写本地）：用于激活前预览。
#[tauri::command]
pub async fn verify_license_remote(
    app: AppHandle,
    license: State<'_, LicenseService>,
    token: String,
) -> AppResult<LicenseStatus> {
    license.verify_remote_only(&app, &token).await
}

/// 读当前生效的 license server URL（已规范化）。
#[tauri::command]
pub async fn get_license_server_url(app: AppHandle, license: State<'_, LicenseService>) -> AppResult<Option<String>> {
    license.server_url(&app).await
}

#[tauri::command]
pub async fn activate_license(app: AppHandle, license: State<'_, LicenseService>, token: String) -> AppResult<LicenseStatus> {
    license.activate(&app, &token).await
}

#[tauri::command]
pub async fn remove_license(app: AppHandle, license: State<'_, LicenseService>) -> AppResult<LicenseStatus> {
    license.remove(&app).await
}

/// 在线模式「替换绑定」：把当前 license 的 server 端绑定替换到本机。
/// 前置条件：本机已激活该 license（license.json 存在）。
/// 行为细节见 `LicenseService::rebind_online`。
#[tauri::command]
pub async fn replace_license_binding(
    app: AppHandle,
    license: State<'_, LicenseService>,
) -> AppResult<LicenseStatus> {
    license.rebind_online(&app).await
}
