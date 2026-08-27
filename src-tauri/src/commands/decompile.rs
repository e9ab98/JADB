use crate::config::settings;
use crate::error::{AppError, AppResult};
use crate::services::apk_decompiler::{self, TaskHandle};
use crate::services::task_registry::TaskRegistry;
use std::process::Stdio;
use tauri::{AppHandle, Manager, State};
use tokio::process::Command;

/// Resolve a non-conflicting output directory for apktool decompile.
///
/// The system directory picker only lets the user pick an existing
/// path, but apktool refuses any pre-existing output directory unless
/// `-f` is passed. To keep the form's "force overwrite" toggle
/// semantically meaningful (i.e. only set when the user genuinely
/// wants to clobber an existing project), the frontend calls this
/// command after the user picks a parent directory and turns it into
/// `<parent>/<apk-basename>` — or `<parent>/<apk-basename>_1`,
/// `_2`, … if that already exists from a prior run. The path is
/// returned verbatim; apktool creates the directory on its own when
/// it runs. This command does NOT touch the filesystem.
#[tauri::command]
pub async fn resolve_unique_out_dir(parent: String, base_name: String) -> AppResult<String> {
    let parent_path = std::path::Path::new(parent.trim());
    if parent.trim().is_empty() || !parent_path.is_dir() {
        return Err(AppError::InvalidInput(format!(
            "parent is not an existing directory: {parent}"
        )));
    }
    // Sanitize: strip trailing .apk (any case) but keep any other
    // characters the user had in the original file name. We don't try
    // to clean up arbitrary characters — the OS / apktool will surface
    // a clearer error than we could fabricate here if the name is
    // unworkable.
    let base = std::path::Path::new(base_name.trim())
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| {
            let s = s.trim();
            s.strip_suffix(".apk")
                .or_else(|| s.strip_suffix(".APK"))
                .unwrap_or(s)
        })
        .unwrap_or("decompiled")
        .to_string();
    if base.is_empty() {
        return Err(AppError::InvalidInput("base_name is empty".into()));
    }
    let primary = parent_path.join(&base);
    if !primary.exists() {
        return Ok(primary.to_string_lossy().into_owned());
    }
    for i in 1..=999 {
        let candidate = parent_path.join(format!("{base}_{i}"));
        if !candidate.exists() {
            return Ok(candidate.to_string_lossy().into_owned());
        }
    }
    Err(AppError::Config(format!(
        "无法在 {parent} 下为 {base} 找到可用目录（已尝试 999 个后缀）"
    )))
}

#[tauri::command]
pub async fn decompile_apk(
    app: AppHandle,
    registry: State<'_, TaskRegistry>,
    path: String,
    out_dir: String,
    force: bool,
) -> AppResult<TaskHandle> {
    if path.trim().is_empty() {
        return Err(AppError::InvalidInput("path is empty".into()));
    }
    if out_dir.trim().is_empty() {
        return Err(AppError::InvalidInput("out_dir is empty".into()));
    }
    if !std::path::Path::new(&path).exists() {
        return Err(AppError::InvalidInput(format!("file not found: {path}")));
    }
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let settings = settings::read(&dir).await?;
    apk_decompiler::decompile(&app, &registry, &settings, &path, &out_dir, force).await
}

#[tauri::command]
pub async fn open_path(path: String) -> AppResult<()> {
    if path.trim().is_empty() {
        return Err(AppError::InvalidInput("path is empty".into()));
    }
    // Spawn the OS handler. `open` on macOS, `xdg-open` on Linux, `cmd /c start ""` on Windows.
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = Command::new("open");
        c.arg(&path);
        c
    };
    #[cfg(target_os = "linux")]
    let mut cmd = {
        let mut c = Command::new("xdg-open");
        c.arg(&path);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = Command::new("cmd");
        c.arg("/c").arg("start").arg("").arg(&path);
        c
    };
    cmd.stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| AppError::Config(format!("open path failed: {e}")))?;
    Ok(())
}
