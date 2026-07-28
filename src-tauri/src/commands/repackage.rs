use crate::config::settings;
use crate::error::{AppError, AppResult};
use crate::services::apk_repackager::{self, TaskHandle};
use crate::services::apk_signer::SigningSchemes;
use crate::services::task_registry::TaskRegistry;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

#[tauri::command]
pub async fn repackage_apk(
    app: AppHandle,
    registry: State<'_, TaskRegistry>,
    src_dir: String,
    out_apk: String,
    sign: bool,
    signature_id: Option<String>,
    schemes: SigningSchemes,
) -> AppResult<TaskHandle> {
    if src_dir.trim().is_empty() {
        return Err(AppError::InvalidInput("src_dir is empty".into()));
    }
    if out_apk.trim().is_empty() {
        return Err(AppError::InvalidInput("out_apk is empty".into()));
    }
    let src_path = PathBuf::from(&src_dir);
    if !src_path.exists() {
        return Err(AppError::InvalidInput(format!("src_dir not found: {src_dir}")));
    }
    if !src_path.join("AndroidManifest.xml").exists() && !src_path.join("apktool.yml").exists() {
        return Err(AppError::InvalidInput(
            "src_dir does not look like an apktool project (no AndroidManifest.xml or apktool.yml)"
                .into(),
        ));
    }
    // Ensure the parent directory of out_apk exists.
    if let Some(parent) = PathBuf::from(&out_apk).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AppError::Config(format!("create out dir: {e}")))?;
        }
    }
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let settings = settings::read(&dir).await?;
    apk_repackager::repackage(
        &app,
        &registry,
        &settings,
        &src_dir,
        &out_apk,
        sign,
        signature_id,
        schemes,
    )
    .await
}
