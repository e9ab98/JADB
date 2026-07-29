use crate::config::settings;
use crate::error::{AppError, AppResult};
use crate::services::apk_signature::{self, SignatureInfo};
use crate::services::apk_signer::{self, SigningSchemes, TaskHandle};
use crate::services::task_registry::TaskRegistry;
use crate::services::license::{LicenseService, FEATURE_SIGNING_V31};
use serde::Deserialize;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "mode")]
pub enum SignRequest {
    Standard {
        apk_path: String,
        signature_id: String,
        allow_resign: bool,
        schemes: SigningSchemes,
    },
    Rotation {
        apk_path: String,
        lineage_id: String,
        allow_resign: bool,
        v4_enabled: bool,
    },
}

#[tauri::command]
pub async fn check_apk_signed(apk_path: String) -> AppResult<bool> {
    if apk_path.trim().is_empty() {
        return Err(AppError::InvalidInput("apk_path is empty".into()));
    }
    let path = PathBuf::from(apk_path);
    tokio::task::spawn_blocking(move || apk_signature::is_apk_signed(&path))
        .await
        .map_err(|error| AppError::Config(format!("signature check task failed: {error}")))?
}

async fn ensure_app_path(appk: &str) -> AppResult<()> {
    if appk.trim().is_empty() {
        return Err(AppError::InvalidInput("apk_path is empty".into()));
    }
    if !PathBuf::from(appk).exists() {
        return Err(AppError::InvalidInput(format!("file not found: {appk}")));
    }
    Ok(())
}

async fn ensure_not_signed(appk: &str, allow_resign: bool) -> AppResult<()> {
    if allow_resign {
        return Ok(());
    }
    let path = PathBuf::from(appk);
    let signed = tokio::task::spawn_blocking(move || apk_signature::is_apk_signed(&path))
        .await
        .map_err(|error| AppError::Config(format!("signature check task failed: {error}")))??;
    if signed {
        return Err(AppError::InvalidInput(
            "APK 已有签名,需要确认后才能重新签名".into(),
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn sign_apk(
    app: AppHandle,
    registry: State<'_, TaskRegistry>,
    license: State<'_, LicenseService>,
    request: SignRequest,
) -> AppResult<TaskHandle> {
    match request {
        SignRequest::Standard {
            apk_path,
            signature_id,
            allow_resign,
            schemes,
        } => {
            if signature_id.trim().is_empty() {
                return Err(AppError::InvalidInput("signature_id is empty".into()));
            }
            schemes.validate()?;
            ensure_app_path(&apk_path).await?;
            ensure_not_signed(&apk_path, allow_resign).await?;
            let dir = app
                .path()
                .app_data_dir()
                .map_err(|e| AppError::Config(e.to_string()))?;
            let settings = settings::read(&dir).await?;
            apk_signer::sign(
                &app,
                &registry,
                &settings,
                &apk_path,
                &signature_id,
                schemes,
            )
            .await
        }
        SignRequest::Rotation {
            apk_path,
            lineage_id,
            allow_resign,
            v4_enabled,
        } => {
            license.require_feature(&app, FEATURE_SIGNING_V31).await?;
            if lineage_id.trim().is_empty() {
                return Err(AppError::InvalidInput("lineage_id is empty".into()));
            }
            ensure_app_path(&apk_path).await?;
            ensure_not_signed(&apk_path, allow_resign).await?;
            let dir = app
                .path()
                .app_data_dir()
                .map_err(|e| AppError::Config(e.to_string()))?;
            let settings = settings::read(&dir).await?;
            apk_signer::sign_rotation(
                &app,
                &registry,
                &settings,
                &apk_path,
                &lineage_id,
                v4_enabled,
            )
            .await
        }
    }
}

#[tauri::command]
pub async fn inspect_signature(app: AppHandle, apk_path: String) -> AppResult<SignatureInfo> {
    apk_signature::inspect_signature(&app, &apk_path).await
}

#[tauri::command]
pub async fn strip_apk_signing(
    app: AppHandle,
    registry: State<'_, TaskRegistry>,
    apk_path: String,
    output_path: Option<String>,
) -> AppResult<TaskHandle> {
    apk_signature::strip_signing(&app, &registry, &apk_path, output_path.as_deref()).await
}
