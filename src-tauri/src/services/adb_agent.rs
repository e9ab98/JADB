//! On-device agent for fast package metadata extraction.
//!
//! Pushes a tiny dex (`assets/adbhelper/adbhelper.dex`) to `/data/local/tmp/`
//! and runs it via `app_process -D java.class.path=<dex>`. The agent uses Android's hidden
//! `IPackageManager` binder directly, so a 200-app device dumps in ~3s
//! instead of the old `pm list + dumpsys + pm path + wc + pull + aapt2`
//! cascade.
//!
//! Each package line on stdout is a single JSON object:
//!   {"package":"com.foo","apkPath":"/data/app/.../base.apk","apkSize":12345,
//!    "label":"Foo","hasIcon":true,"flags":1,"enabled":true,
//!    "versionCode":100,"versionName":"1.0","firstInstallTime":...,...,
//!    "dataDir":"/data/data/com.foo","minSdkVersion":24,"targetSdkVersion":34,
//!    "mainActivity":"com.foo.MainActivity"}
//!
//! Icons are written to `<args[0]>/icons/<pkg>.png` (PNG, ~20% quality)
//! and pulled back individually by `pull_icon`.

use crate::config::settings::Settings;
use crate::error::{AppError, AppResult};
use crate::services::adb_manager::{run_adb, AppInfo};
use serde_json::Value;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// Embedded dex. The agent's main class is `com.e9ab98.adbhelper.Server`.
const AGENT_DEX: &[u8] = include_bytes!("../../assets/adbhelper/adbhelper.dex");

/// On-device path for the staged agent (single file, ~13 KB).
pub const AGENT_REMOTE_PATH: &str = "/data/local/tmp/adbhelper.dex";

/// On-device directory the agent writes icons into. The agent appends
/// `/icons` internally, so the parent directory is what we pass as
/// `args[0]`.
pub const AGENT_ICON_ROOT: &str = "/data/local/tmp/jadb-icons";
pub const AGENT_ICON_DIR: &str = "/data/local/tmp/jadb-icons/icons";

/// Main class to invoke (must match the class with `main(String[])`).
const AGENT_MAIN_CLASS: &str = "com.e9ab98.adbhelper.Server";

/// Make sure the dex is present on the device. Push is skipped when the
/// remote file already exists with the same size, so this is cheap to
/// call from every request.
pub async fn ensure_agent_pushed(app: &AppHandle, settings: &Settings, device: &str) -> AppResult<()> {
    let local = stage_local_dex(app)?;
    let local_size = tokio::fs::metadata(&local).await.map_err(AppError::Io)?.len();

    // Probe remote size. `stat -c %s` prints just the integer; failure means
    // the file is missing or unreadable — in both cases push it.
    let probe = run_adb(
        settings,
        Some(device),
        &["shell", "stat", "-c", "%s", AGENT_REMOTE_PATH],
    )
    .await;
    let need_push = match probe {
        Ok(s) => s.trim().parse::<u64>().ok() != Some(local_size),
        Err(_) => true,
    };
    if need_push {
        let local_str = local.to_string_lossy().into_owned();
        let _ = run_adb(settings, Some(device), &["push", &local_str, AGENT_REMOTE_PATH]).await?;
    }
    // Make sure the icon dir exists with permissive perms — the agent
    // runs as shell uid and writes PNGs into it.
    let _ = run_adb(
        settings,
        Some(device),
        &["shell", "mkdir", "-p", AGENT_ICON_DIR],
    )
    .await;
    let _ = run_adb(
        settings,
        Some(device),
        &["shell", "chmod", "777", AGENT_ICON_ROOT],
    )
    .await;
    Ok(())
}

/// Materialise the embedded dex to a stable cache path. We always rewrite
/// the local file from `include_bytes!` so that updates to the asset are
/// picked up after a rebuild without manual cache eviction.
fn stage_local_dex(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| AppError::Config(e.to_string()))?
        .join("adbhelper");
    let path = dir.join("adbhelper.dex");
    std::fs::create_dir_all(&dir).map_err(AppError::Io)?;
    std::fs::write(&path, AGENT_DEX).map_err(AppError::Io)?;
    Ok(path)
}

/// Run the agent and return its raw stdout (one JSON object per line,
/// with a leading `====loading N packages====` banner).
/// Run the agent with custom `args` and return its raw stdout.
///
/// Pass `&[]` for the default package-list mode (Server.java dispatches
/// `args.length == 1` to `dumpPackageInfo` with the icon dir, which is
/// always `AGENT_ICON_ROOT` from our side). Pass `&["sysinfo"]` for the
/// system-info dump mode (Server.java dispatches to `SystemInfoDumper.dump`).
///
/// The args layout is `app_process -classpath <dex> /system/bin <main>
/// [args...]`. `/system/bin` is the app data dir placeholder; it has to be
/// a real directory that exists on every Android build.
pub async fn run_agent(settings: &Settings, device: &str, extra_args: &[&str]) -> AppResult<String> {
    let extra = extra_args.iter().map(|a| shell_quote(a)).collect::<Vec<_>>().join(" ");
    let cmd = if extra.is_empty() {
        format!(
            "app_process -classpath {} /system/bin {} {}",
            AGENT_REMOTE_PATH, AGENT_MAIN_CLASS, AGENT_ICON_ROOT,
        )
    } else {
        format!(
            "app_process -classpath {} /system/bin {} {} {}",
            AGENT_REMOTE_PATH, AGENT_MAIN_CLASS, AGENT_ICON_ROOT, extra,
        )
    };
    run_adb(settings, Some(device), &["shell", &cmd]).await
}

/// POSIX-ish single-quote escape for shell arg interpolation.
fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    if value.chars().all(|c| {
        c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '+' | '@' | '=')
    }) {
        return value.to_string();
    }
    format!("'{}'", value.replace('\\', r"\\").replace('\'', r"\'"))
}

/// Parse the agent's stdout into `AppInfo` rows. Lines that don't look
/// like JSON (the `====loading ...====` banner, empty lines, etc.) are
/// silently skipped.
pub fn parse_agent_output(stdout: &str) -> Vec<AppInfo> {
    let mut out = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || !trimmed.starts_with('{') {
            continue;
        }
        if let Some(info) = parse_agent_line(trimmed) {
            out.push(info);
        }
    }
    out
}

/// One row of agent stdout → `AppInfo`.
fn parse_agent_line(line: &str) -> Option<AppInfo> {
    let v: Value = serde_json::from_str(line).ok()?;
    let obj = v.as_object()?;

    let package_name = obj.get("package")?.as_str()?.to_string();
    let flags = obj.get("flags").and_then(Value::as_u64).unwrap_or(0);
    // ApplicationInfo.FLAG_SYSTEM = 0x00000001, FLAG_DEBUGGABLE = 0x00000002
    let is_system = (flags & 0x1) != 0;
    let is_debuggable = (flags & 0x2) != 0;

    let apk_path = obj
        .get("apkPath")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let apk_total_size = obj.get("apkSize").and_then(Value::as_u64);
    let has_icon = obj.get("hasIcon").and_then(Value::as_bool).unwrap_or(false);

    // If the agent rendered an icon, fill `icon_path` with the remote PNG
    // location so the existing icon-pulling worker can find it without
    // re-deriving the path.
    let icon_path = if has_icon {
        Some(format!("{}/{}.png", AGENT_ICON_DIR, package_name))
    } else {
        None
    };

    let num_to_string = |k: &str| -> Option<String> {
        obj.get(k)
            .and_then(Value::as_i64)
            .map(|n| n.to_string())
            .or_else(|| obj.get(k).and_then(Value::as_u64).map(|n| n.to_string()))
    };

    Some(AppInfo {
        package_name,
        app_label: obj
            .get("label")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
        version_name: obj
            .get("versionName")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
        version_code: num_to_string("versionCode"),
        min_sdk: obj
            .get("minSdkVersion")
            .and_then(Value::as_u64)
            .map(|n| n.to_string()),
        target_sdk: obj
            .get("targetSdkVersion")
            .and_then(Value::as_u64)
            .map(|n| n.to_string()),
        apk_path,
        apk_total_size,
        // The agent doesn't enumerate split APKs, so we report 1 when it
        // saw an apkPath and 0 otherwise. The icon worker still pulls
        // individual icons, so this only affects the displayed counter.
        apk_count: if obj.get("apkPath").is_some() { 1 } else { 0 },
        icon_path,
        // Populated lazily by `pull_icon`.
        icon_data_url: None,
        is_system,
        is_debuggable,
    })
}

/// Pull a single icon PNG and return it as a `data:image/png;base64,...`
/// URL. Returns `Ok(None)` when the agent didn't render an icon for the
/// package (or when the remote file is missing — e.g. user uninstalled
/// the app between the dump and the pull).
pub async fn pull_icon(
    settings: &Settings,
    device: &str,
    package: &str,
) -> AppResult<Option<String>> {
    if package.is_empty() || package.contains('/') || package.contains("..") {
        return Err(AppError::InvalidInput(format!("invalid package name: {package}")));
    }
    let remote = format!("{}/{}.png", AGENT_ICON_DIR, package);
    // Use the bytes-preserving helper -- PNG is binary and `run_adb`
    // (which goes through `String::from_utf8_lossy`) would corrupt any
    // high-byte pixel by replacing it with U+FFFD, making the icon
    // undecodable on the frontend.
    let raw = crate::services::adb_manager::run_adb_bytes(
        settings,
        Some(device),
        &["exec-out", "cat", &remote],
    )
    .await;
    let bytes = match raw {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };
    if bytes.is_empty() {
        return Ok(None);
    }
    // Heuristic: PNGs start with the 8-byte signature
    // `89 50 4E 47 0D 0A 1A 0A`. If we got stderr noise instead, bail.
    const PNG_SIG: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    if bytes.len() < PNG_SIG.len() || bytes[..PNG_SIG.len()] != PNG_SIG {
        return Ok(None);
    }
    let b64 = crate::services::adb_manager::base64_encode(&bytes);
    Ok(Some(format!("data:image/png;base64,{}", b64)))
}

/// Run the agent in sysinfo mode and return the raw JSON object as a
/// generic `Value`. Caller is responsible for merging fields into
/// `DeviceSystemInfo` -- shell fallback still covers anything the agent
/// couldn't fill in (CPU details, kernel version, foreground app,
/// storage, etc).
pub async fn run_agent_sysinfo(
    settings: &Settings,
    device: &str,
) -> AppResult<serde_json::Value> {
    let stdout = run_agent(settings, device, &["sysinfo"]).await?;
    // The agent emits a single line of JSON. Pick the first `{`-prefixed
    // line (skipping the `====...====` banner if any) and parse it.
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || !trimmed.starts_with('{') {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            return Ok(v);
        }
    }
    Err(AppError::Parse(format!(
        "agent sysinfo: no JSON object in output ({} bytes)",
        stdout.len()
    )))
}
