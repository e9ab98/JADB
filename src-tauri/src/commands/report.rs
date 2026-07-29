use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, State};
use crate::services::license::{LicenseService, FEATURE_REPORT_EXPORT};

/// Frontend hands us the entire rendered report payload as a string —
/// the actual HTML template lives in `src/features/apkAnalyze/reportTemplate.ts`
/// where the React/i18n layer owns the visual. We just take whatever the
/// renderer produces and stream it to disk. This keeps the report look
/// consistent with the in-app dashboard without re-implementing styling
/// in Rust.
#[derive(Deserialize, Debug)]
pub struct ExportReportArgs {
    pub dest_path: String,
    pub html: String,
}

#[derive(Serialize, Debug)]
pub struct ExportReportResult {
    pub dest_path: String,
    pub bytes_written: u64,
}

#[tauri::command]
pub async fn export_apk_report(
    app: AppHandle,
    license: State<'_, LicenseService>,
    args: ExportReportArgs,
) -> AppResult<ExportReportResult> {
    license.require_feature(&app, FEATURE_REPORT_EXPORT).await?;
    let dest = PathBuf::from(&args.dest_path);
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(AppError::InvalidInput(format!(
                "destination directory does not exist: {}",
                parent.display()
            )));
        }
    }
    if args.html.is_empty() {
        return Err(AppError::InvalidInput("report html is empty".into()));
    }

    let bytes = args.html.as_bytes();
    let len = bytes.len() as u64;
    tokio::fs::write(&dest, bytes)
        .await
        .map_err(AppError::Io)?;

    Ok(ExportReportResult {
        dest_path: args.dest_path,
        bytes_written: len,
    })
}
