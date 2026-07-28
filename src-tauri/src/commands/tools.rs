use crate::config::tools::ToolName;
use crate::error::{AppError, AppResult};
use crate::services::tool_manager::{self, ToolStatus};
use std::str::FromStr;
use tauri::{AppHandle, Manager};

#[tauri::command]
pub async fn get_tool_status(app: AppHandle) -> AppResult<Vec<ToolStatus>> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = crate::config::settings::read(&dir).await?;
    Ok(tool_manager::status_all(&dir, &s).await)
}

#[tauri::command]
pub async fn install_tool(app: AppHandle, name: String) -> AppResult<ToolStatus> {
    let tool = ToolName::from_str(&name).map_err(AppError::Config)?;
    tool_manager::install(&app, tool).await
}

#[tauri::command]
pub async fn remove_tool(app: AppHandle, name: String) -> AppResult<()> {
    let tool = ToolName::from_str(&name).map_err(AppError::Config)?;
    tool_manager::remove(&app, tool).await
}
