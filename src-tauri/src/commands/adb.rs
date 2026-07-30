use crate::config::settings;
use crate::error::{AppError, AppResult};
use crate::services::adb_manager::{self, AdbDevice, AppInfo, DeviceSystemInfo, DirEntry, ExportApksResult, InstallApksResult};
use tauri::{AppHandle, Manager, State};
use crate::services::license::{LicenseService, FEATURE_ADB_BATCH_INSTALL};

#[tauri::command]
pub async fn adb_devices(app: AppHandle) -> AppResult<Vec<AdbDevice>> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::list_devices(&s).await
}

#[tauri::command]
pub async fn adb_connect(
    app: AppHandle,
    host: String,
    port: u16,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::connect_wifi(&s, &host, port).await
}

#[tauri::command]
pub async fn adb_disconnect(
    app: AppHandle,
    target: Option<String>,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::disconnect(&s, target.as_deref()).await
}

#[tauri::command]
pub async fn adb_list_packages(
    app: AppHandle,
    device: String,
    include_system: bool,
) -> AppResult<Vec<String>> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::list_packages(&s, &device, include_system).await
}

#[tauri::command]
pub async fn adb_app_info(
    app: AppHandle,
    device: String,
    package: String,
) -> AppResult<AppInfo> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::package_info(&app, &s, &device, &package).await
}

#[tauri::command]
pub async fn adb_app_icon(
    app: AppHandle,
    device: String,
    package: String,
) -> AppResult<Option<String>> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::pull_app_icon(&app, &s, &device, &package).await
}

#[tauri::command]
pub async fn adb_install_apks(
    app: AppHandle,
    license: State<'_, LicenseService>,
    device: String,
    paths: Vec<String>,
) -> AppResult<InstallApksResult> {
    if paths.len() > 1 {
        license.require_feature(&app, FEATURE_ADB_BATCH_INSTALL).await?;
    }
    let dir = app.path().app_data_dir().map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::install_apks(&s, &device, &paths).await
}

#[tauri::command]
pub async fn adb_uninstall(
    app: AppHandle,
    device: String,
    package: String,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::uninstall(&s, &device, &package).await
}


#[tauri::command]
pub async fn adb_apk_paths(app: AppHandle, device: String, package: String) -> AppResult<Vec<String>> {
    let dir = app.path().app_data_dir().map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::apk_paths_for(&s, &device, &package).await
}

#[tauri::command]
pub async fn adb_pull_apk_for_tool(app: AppHandle, device: String, package: String, remote_path: String) -> AppResult<String> {
    let dir = app.path().app_data_dir().map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::pull_apk_to_cache(&app, &s, &device, &package, &remote_path).await
}

#[tauri::command]
pub async fn adb_export_apks(
    app: AppHandle,
    device: String,
    package: String,
    version_name: Option<String>,
    target_dir: String,
) -> AppResult<ExportApksResult> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| AppError::Config(error.to_string()))?;
    let settings = settings::read(&dir).await?;
    adb_manager::export_apks(
        &settings,
        &device,
        &package,
        version_name.as_deref(),
        &target_dir,
    )
    .await
}

#[tauri::command]
pub async fn adb_force_stop(
    app: AppHandle,
    device: String,
    package: String,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::force_stop(&s, &device, &package).await
}

#[tauri::command]
pub async fn adb_launch_app(
    app: AppHandle,
    device: String,
    package: String,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::launch_app(&s, &device, &package).await
}

#[tauri::command]
pub async fn adb_clear_cache(
    app: AppHandle,
    device: String,
    package: String,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::clear_cache(&s, &device, &package).await
}

#[tauri::command]
pub async fn is_device_rooted(
    app: AppHandle,
    device: String,
) -> AppResult<bool> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::is_device_rooted(&s, &device).await
}

#[tauri::command]
pub async fn pull_file(
    app: AppHandle,
    device: String,
    remote_path: String,
    local_path: String,
    as_pkg: Option<String>,
    use_root: bool,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::download_file(
        &s,
        &device,
        &remote_path,
        &local_path,
        as_pkg.as_deref(),
        use_root,
    )
    .await
}

#[tauri::command]
pub async fn list_remote_dir(
    app: AppHandle,
    device: String,
    path: String,
    as_pkg: Option<String>,
    use_root: bool,
) -> AppResult<Vec<DirEntry>> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::list_remote_dir(&s, &device, &path, as_pkg.as_deref(), use_root).await
}

#[tauri::command]
pub async fn resolve_app_data_dir(
    app: AppHandle,
    device: String,
    package: String,
    as_pkg: Option<String>,
    use_root: bool,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::resolve_app_data_dir(
        &s,
        &device,
        &package,
        as_pkg.as_deref(),
        use_root,
    )
    .await
}

#[tauri::command]
pub async fn delete_remote_file(
    app: AppHandle,
    device: String,
    path: String,
    as_pkg: Option<String>,
    use_root: bool,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::delete_remote_file(&s, &device, &path, as_pkg.as_deref(), use_root).await
}

#[tauri::command]
pub async fn push_file(
    app: AppHandle,
    device: String,
    local_path: String,
    remote_path: String,
    as_pkg: Option<String>,
    use_root: bool,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::push_file(
        &s,
        &device,
        &local_path,
        &remote_path,
        as_pkg.as_deref(),
        use_root,
    )
    .await
}

use crate::services::adb_manager::ShellOutput;

#[tauri::command]
pub async fn adb_shell(
    app: AppHandle,
    device: String,
    command: String,
) -> AppResult<ShellOutput> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::shell_exec(&s, &device, &command).await
}


#[tauri::command]
pub async fn adb_system_info(
    app: AppHandle,
    device: String,
) -> AppResult<DeviceSystemInfo> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::system_info(&s, &device).await
}
