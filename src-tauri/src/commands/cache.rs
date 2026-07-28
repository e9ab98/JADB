use crate::error::AppResult;
use crate::services::cache::{self, CacheClearResult, CacheScanResult};
use tauri::AppHandle;

#[tauri::command]
pub async fn scan_cache(app: AppHandle) -> AppResult<CacheScanResult> {
    cache::scan(&app).await
}

#[tauri::command]
pub async fn clear_cache(app: AppHandle) -> AppResult<CacheClearResult> {
    cache::clear_via_app(&app).await
}
