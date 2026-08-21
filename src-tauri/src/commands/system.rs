use crate::error::{AppError, AppResult};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tauri::{AppHandle, Manager};

/// JSON shape returned by [`get_app_version`]. Kept tiny so the
/// frontend can early-bail when something goes wrong instead of
/// having to unwrap a `serde_json::Value`.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AppVersionInfo {
    /// Raw version string from `Cargo.toml` (e.g. `"0.1.29"`).
    pub version: String,
    /// Build profile, e.g. `"debug"` or `"release"`. Useful for
    /// debug fixtures to render a "DEV" pill next to the version
    /// chip; in release builds it should always be `"release"`.
    pub profile: String,
    /// Version Tauri ships with, straight from `tauri.conf.json`.
    /// We surface it so the frontend can render the same branding
    /// string that the OS-level window title uses.
    pub tauri_version: String,
}

/// Return the application version + build metadata. The Rust side
/// reads:
///   * `CARGO_PKG_VERSION` (compile-time, guaranteed to match the
///     version baked into the release binary).
///   * `cfg!(debug_assertions)` (compile-time, debug vs release).
///   * `app.config().version` (the same string Tauri uses to
///     construct the `Bundle`).
///
/// We never fall back to a hardcoded string — if the build context
/// is missing the version field we fall back to `CARGO_PKG_VERSION`
/// so the frontend can always render *something* accurate instead
/// of silently showing a stale version.
#[tauri::command]
pub async fn get_app_version(app: AppHandle) -> AppResult<AppVersionInfo> {
    let pkg_version = env!("CARGO_PKG_VERSION").to_string();
    let profile = if cfg!(debug_assertions) {
        "debug".to_string()
    } else {
        "release".to_string()
    };
    let tauri_version = app
        .config()
        .version
        .clone()
        .unwrap_or_else(|| pkg_version.clone());
    Ok(AppVersionInfo {
        version: pkg_version,
        profile,
        tauri_version,
    })
}

/// Pick the most-recently-modified file inside `dir`. Returns `dir` itself when
/// the directory is empty so callers can still surface a path.
pub fn latest_file_in(dir: &Path) -> AppResult<PathBuf> {
    let entries = std::fs::read_dir(dir).map_err(AppError::Io)?;
    let mut files: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    if files.is_empty() {
        return Ok(dir.to_path_buf());
    }
    files.sort_by_key(|e| {
        e.metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .unwrap_or(SystemTime::UNIX_EPOCH)
    });
    Ok(files.last().expect("non-empty checked above").path())
}

#[tauri::command]
pub async fn get_log_path(app: AppHandle) -> AppResult<String> {
    let dir = app
        .path()
        .app_log_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    if !dir.exists() {
        return Err(AppError::NotFound(format!(
            "log directory does not exist yet: {}",
            dir.display()
        )));
    }
    Ok(latest_file_in(&dir)?.to_string_lossy().to_string())
}
