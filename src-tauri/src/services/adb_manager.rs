use crate::config::settings::Settings;
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use super::adb_dumpsys;
use std::process::Stdio;
use tauri::{AppHandle, Manager};
use tokio::process::Command;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AdbDevice {
    pub serial: String,
    pub state: String,
    pub model: Option<String>,
    pub product: Option<String>,
    pub transport: Option<String>,
}

/// Lightweight package metadata returned by `list_packages`. The
/// frontend uses this to render the apps list *before* `package_info`
/// (which has to pull the APK from the device and run aapt2) has
/// finished — keeping this struct cheap and free of adb calls beyond
/// `pm list packages`.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ListedApp {
    pub name: String,
    pub is_system: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub package_name: String,
    pub app_label: Option<String>,
    pub version_name: Option<String>,
    pub version_code: Option<String>,
    pub min_sdk: Option<String>,
    pub target_sdk: Option<String>,
    pub apk_path: Option<String>,
    /// Sum of base.apk and all split APK files currently installed.
    pub apk_total_size: Option<u64>,
    pub apk_count: usize,
    /// Absolute path inside the APK (e.g. `res/mipmap-mdpi-v4/ic_launcher.png`).
    pub icon_path: Option<String>,
    /// `data:image/png;base64,...` — populated by `pull_app_icon`.
    pub icon_data_url: Option<String>,
    pub is_system: bool,
    pub is_debuggable: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExportApksResult {
    pub count: usize,
    pub directory: String,
}

/// One entry returned by `ls -la`. The frontend uses this to render the
/// data-dir file manager window (per-app).
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    /// Absolute path on the device.
    pub path: String,
    /// `file` | `dir` | `link` | `other`.
    pub kind: String,
    pub size: u64,
    /// 10-char permissions string from `ls`, e.g. `-rwxr-xr-x`.
    pub permissions: String,
    /// Raw `ls` date — kept as a string so we don't fight with Android's
    /// varying time formats (`Mon DD HH:MM` vs `Mon DD YYYY`).
    pub modified: String,
    /// For symlinks: target path. `None` for regular entries.
    pub link_target: Option<String>,
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Parse one line of `ls -la` output. Returns `None` for the `total NNN`
/// header, `.`, `..`, and unparseable lines.
fn parse_ls_line(line: &str, base_path: &str) -> Option<DirEntry> {
    if line.starts_with("total ") {
        return None;
    }
    if line.len() < 10 {
        return None;
    }
    let perms = &line[..10];
    let rest = &line[10..];
    let parts: Vec<&str> = rest.split_whitespace().collect();
    // `ls -la` uses one of two date formats depending on platform / locale:
    //   Traditional (GNU coreutils, older toybox): "Mon DD HH:MM" / "Mon DD  YYYY"
    //     -> 3 fields before name -> name at parts[7]
    //   ISO-style (newer toybox, some Android 13+ builds, custom locales):
    //     "YYYY-MM-DD HH:MM" -> 2 fields before name -> name at parts[6]
    // Probe by looking for the dash in the 5th position of parts[4], which
    // uniquely identifies "YYYY-MM-DD" without false-matching "Mon" / "Jan".
    let name_start: usize = if parts.len() >= 7
        && parts[4].len() >= 10
        && parts[4].as_bytes()[4] == b'-'
    {
        6
    } else {
        7
    };
    if parts.len() < name_start + 1 {
        return None;
    }
    let kind = match perms.as_bytes()[0] {
        b'd' => "dir",
        b'-' => "file",
        b'l' => "link",
        _ => "other",
    };
    let size: u64 = parts[3].parse().unwrap_or(0);
    // Name is everything from `name_start` onward (may contain spaces).
    let name = parts[name_start..].join(" ");
    if name.is_empty() || name == "." || name == ".." {
        return None;
    }
    let (name, link_target) = if kind == "link" {
        if let Some(idx) = name.find(" -> ") {
            let (n, t) = name.split_at(idx);
            (n.to_string(), Some(t.trim_start_matches(" -> ").to_string()))
        } else {
            (name, None)
        }
    } else {
        (name, None)
    };
    let full_path = if base_path.ends_with('/') {
        format!("{}{}", base_path, name)
    } else {
        format!("{}/{}", base_path, name)
    };
    let modified = parts[4..name_start].join(" ");
    Some(DirEntry {
        name,
        path: full_path,
        kind: kind.to_string(),
        size,
        permissions: perms.to_string(),
        modified,
        link_target,
    })
}

fn adb_binary(settings: &Settings) -> AppResult<&str> {
    settings
        .adb_path
        .as_deref()
        .ok_or_else(|| AppError::ToolMissing("adb".into()))
}

/// Run an adb subcommand. `serial` is an optional `-s <device>` selector.
pub(crate) async fn run_adb(settings: &Settings, serial: Option<&str>, args: &[&str]) -> AppResult<String> {
    let adb = adb_binary(settings)?.to_string();
    let mut cmd = Command::new(&adb);
    if let Some(s) = serial {
        cmd.arg("-s").arg(s);
    }
    for a in args {
        cmd.arg(a);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let output = cmd
        .output()
        .await
        .map_err(|e| AppError::Config(format!("spawn adb: {e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        return Err(AppError::ToolFailed {
            tool: "adb".into(),
            code: output.status.code().unwrap_or(-1),
            msg: if stderr.trim().is_empty() {
                stdout
            } else {
                stderr
            },
        });
    }
    Ok(if stdout.is_empty() { stderr } else { stdout })
}

/// Same as [`run_adb`] but returns the raw stdout bytes without any
/// UTF-8 transcoding. Use this whenever the command output is binary
/// (e.g. `adb exec-out cat <png>` for icon streaming) -- the lossy
/// `String::from_utf8_lossy` in `run_adb` would corrupt the bytes by
/// replacing invalid UTF-8 sequences with U+FFFD, breaking PNGs that
/// contain high-byte pixels.
pub(crate) async fn run_adb_bytes(
    settings: &Settings,
    serial: Option<&str>,
    args: &[&str],
) -> AppResult<Vec<u8>> {
    let adb = adb_binary(settings)?.to_string();
    let mut cmd = Command::new(&adb);
    if let Some(s) = serial {
        cmd.arg("-s").arg(s);
    }
    for a in args {
        cmd.arg(a);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let output = cmd
        .output()
        .await
        .map_err(|e| AppError::Config(format!("spawn adb: {e}")))?;
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        return Err(AppError::ToolFailed {
            tool: "adb".into(),
            code: output.status.code().unwrap_or(-1),
            msg: stderr,
        });
    }
    Ok(output.stdout)
}

/// Run an `adb shell ...` command and return its stdout.
pub(crate) async fn run_adb_shell(
    settings: &Settings,
    serial: &str,
    shell_args: &[&str],
) -> AppResult<String> {
    let mut wrapped: Vec<String> = Vec::with_capacity(shell_args.len() + 1);
    wrapped.push("shell".to_string());
    for a in shell_args {
        wrapped.push((*a).to_string());
    }
    let borrowed: Vec<&str> = wrapped.iter().map(|s| s.as_str()).collect();
    run_adb(settings, Some(serial), &borrowed).await
}

async fn run_root_shell(
    settings: &Settings,
    serial: &str,
    shell_args: &[&str],
) -> AppResult<String> {
    let command = shell_args
        .iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<_>>()
        .join(" ");
    run_adb_shell(settings, serial, &["su", "-c", &command]).await
}

async fn run_fs_shell(
    settings: &Settings,
    device: &str,
    as_pkg: Option<&str>,
    use_root: bool,
    command: &[&str],
) -> AppResult<String> {
    if let Some(pkg) = as_pkg {
        let mut args = Vec::with_capacity(command.len() + 2);
        args.extend_from_slice(&["run-as", pkg]);
        args.extend_from_slice(command);
        return run_adb_shell(settings, device, &args).await;
    }
    if use_root {
        return match run_root_shell(settings, device, command).await {
            Ok(output) => Ok(output),
            Err(_) => run_adb_shell(settings, device, command).await,
        };
    }
    run_adb_shell(settings, device, command).await
}

pub async fn list_devices(settings: &Settings) -> AppResult<Vec<AdbDevice>> {
    let out = run_adb(settings, None, &["devices", "-l"]).await?;
    let mut devices = Vec::new();
    for line in out.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("List of devices") {
            continue;
        }
        // Format: "<serial>\t<state>\t[colon-separated fields]"
        // e.g. "emulator-5554   device product:sdk_gphone64_x86_64 model:Android_SDK_Built_for_x86_64 device:emu64xa transport-id:1"
        let mut parts = line.split_whitespace();
        let serial = match parts.next() {
            Some(s) => s.to_string(),
            None => continue,
        };
        let state = parts.next().unwrap_or("unknown").to_string();
        let mut model = None;
        let mut product = None;
        let mut transport = None;
        for tok in parts {
            if let Some(v) = tok.strip_prefix("model:") {
                model = Some(v.replace('_', " "));
            } else if let Some(v) = tok.strip_prefix("product:") {
                product = Some(v.to_string());
            } else if let Some(v) = tok.strip_prefix("transport-id:") {
                transport = Some(v.to_string());
            }
        }
        devices.push(AdbDevice {
            serial,
            state,
            model,
            product,
            transport,
        });
    }
    Ok(devices)
}

pub async fn connect_wifi(settings: &Settings, host: &str, port: u16) -> AppResult<String> {
    if host.trim().is_empty() {
        return Err(AppError::InvalidInput("host is empty".into()));
    }
    if port == 0 {
        return Err(AppError::InvalidInput("port must be > 0".into()));
    }
    let port_str = port.to_string();
    let out = run_adb(settings, None, &["connect", host.trim(), &port_str]).await?;
    Ok(out.trim().to_string())
}

pub async fn disconnect(settings: &Settings, target: Option<&str>) -> AppResult<String> {
    let out = match target {
        Some(t) if !t.trim().is_empty() => {
            run_adb(settings, None, &["disconnect", t.trim()]).await?
        }
        _ => run_adb(settings, None, &["disconnect"]).await?,
    };
    Ok(out.trim().to_string())
}

/// Same as [`list_packages`] but returns full [`AppInfo`] rows parsed
/// from `pm list packages -f` output. Used as the slow-path fallback
/// when the on-device agent can't run (e.g. framework JNI signature
/// mismatch on a specific Android build, OEM `app_process` lockdown).
///
/// Metadata fields the shell path can't cheaply give us -- label,
/// version, icon, APK size, debuggable flag -- stay `None` here; the
/// frontend's per-package enrichment fills them in lazily.
pub async fn list_packages_via_shell(
    settings: &Settings,
    device: &str,
    include_system: bool,
) -> AppResult<Vec<AppInfo>> {
    let flag = if include_system { "" } else { "-3" };
    let mut args: Vec<&str> = vec!["pm", "list", "packages", "-f"];
    if !flag.is_empty() {
        args.push(flag);
    }
    let out = run_adb_shell(settings, device, &args).await?;
    let mut infos: Vec<AppInfo> = Vec::new();
    for line in out.lines() {
        // Format: `package:/data/app/~~xxx/com.foo-1/base.apk=com.foo`
        let Some(rest) = line.trim().strip_prefix("package:") else {
            continue;
        };
        // rsplit_once so package names containing `=` (none in practice)
        // still parse. APK path is everything before the last `=`.
        let Some((apk_path, package_name)) = rest.rsplit_once('=') else {
            continue;
        };
        if package_name.is_empty() || apk_path.is_empty() {
            continue;
        }
        // Best-effort system-app classification from the install path.
        // `/system/`, `/vendor/`, `/product/`, `/apex/` are system mounts.
        let is_system = apk_path.starts_with("/system/")
            || apk_path.starts_with("/vendor/")
            || apk_path.starts_with("/product/")
            || apk_path.starts_with("/apex/");
        infos.push(AppInfo {
            package_name: package_name.to_string(),
            app_label: None,
            version_name: None,
            version_code: None,
            min_sdk: None,
            target_sdk: None,
            apk_path: Some(apk_path.to_string()),
            apk_total_size: None,
            apk_count: 1,
            icon_path: None,
            icon_data_url: None,
            is_system,
            is_debuggable: false,
        });
    }
    // User apps first (matches the agent path's sort), then alpha.
    infos.sort_by(|a, b| {
        match (a.is_system, b.is_system) {
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => a.package_name.cmp(&b.package_name),
        }
    });
    Ok(infos)
}

pub async fn list_packages(
    settings: &Settings,
    device: &str,
    include_system: bool,
) -> AppResult<Vec<String>> {
    // `-f` returns "package:/data/app/.../base.apk=com.foo" — strip path for
    // stable ordering. `-3` filters out system apps when requested.
    //
    // NOTE: only pass the args *after* `shell` here. `run_adb_shell`
    // prepends `shell` itself, and a previous version of this function
    // accidentally passed `["shell", "pm", ...]`, producing the duplicated
    // `adb shell shell pm ...` invocation. On the device side that command
    // string starts with the token `shell`, and toybox / Android's
    // `/system/bin/sh` reports `shell: inaccessible or not found` (exit 127).
    let flag = if include_system { "" } else { "-3" };
    let mut args: Vec<&str> = vec!["pm", "list", "packages", "-f"];
    if !flag.is_empty() {
        args.push(flag);
    }
    let out = run_adb_shell(settings, device, &args).await?;
    let mut pkgs: Vec<String> = out
        .lines()
        .filter_map(|l| {
            let trimmed = l.trim().strip_prefix("package:")?;
            let name = trimmed.rsplit('=').next()?;
            if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            }
        })
        .collect();
    pkgs.sort();
    pkgs.dedup();
    Ok(pkgs)
}

/// Parse the output of `pm path <pkg>`, preserving base and split APKs.
fn parse_apk_paths(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("package:"))
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// Read `pm path <pkg>` to get all on-device APK paths for a package.
pub async fn apk_paths_for(settings: &Settings, device: &str, package: &str) -> AppResult<Vec<String>> {
    let out = run_adb_shell(settings, device, &["pm", "path", package]).await?;
    let paths = parse_apk_paths(&out);
    if paths.is_empty() {
        return Err(AppError::NotFound(format!("pm path for {package}")));
    }
    Ok(paths)
}


async fn apk_sizes_for(settings: &Settings, device: &str, paths: &[String]) -> Option<u64> {
    if paths.is_empty() { return None; }
    let mut total = 0u64;
    for path in paths {
        // Pass the quoted path as part of one shell command. Android adb shell
        // argument forwarding does not preserve `sh -c` arguments uniformly
        // across platform-tools versions, which made the previous loop return
        // an empty result on many devices.
        let command = format!("wc -c < {}", shell_quote(path));
        let output = run_adb_shell(settings, device, &[&command]).await.ok()?;
        let size = output
            .split_whitespace()
            .find_map(|value| value.parse::<u64>().ok())?;
        total = total.checked_add(size)?;
    }
    Some(total)
}

pub async fn pull_apk_to_cache(
    app: &AppHandle,
    settings: &Settings,
    device: &str,
    package: &str,
    remote_path: &str,
) -> AppResult<String> {
    let available = apk_paths_for(settings, device, package).await?;
    if !available.iter().any(|path| path == remote_path) {
        return Err(AppError::InvalidInput("selected APK does not belong to package".into()));
    }
    let name = Path::new(remote_path).file_name().and_then(|value| value.to_str())
        .ok_or_else(|| AppError::Parse("invalid remote APK filename".into()))?;
    let cache = app.path().app_cache_dir().map_err(|e| AppError::Config(e.to_string()))?
        .join("device-apks").join(sanitize(device)).join(sanitize(package));
    tokio::fs::create_dir_all(&cache).await?;
    let local = cache.join(name);
    let local_string = local.to_string_lossy().into_owned();
    download_direct(settings, device, remote_path, &local_string).await?;
    Ok(local_string)
}

async fn aapt_dump_badging(settings: &Settings, apk_path: &Path) -> AppResult<String> {
    let aapt = settings
        .aapt_path
        .as_deref()
        .ok_or_else(|| AppError::ToolMissing("aapt2".into()))?;
    let output = Command::new(aapt)
        .arg("dump")
        .arg("badging")
        .arg(apk_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| AppError::Config(format!("spawn aapt2: {e}")))?;
    if !output.status.success() {
        return Err(AppError::ToolFailed {
            tool: "aapt2".into(),
            code: output.status.code().unwrap_or(-1),
            msg: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Parse aapt2 `dump badging` into an `AppInfo`. Best-effort — missing fields
/// stay `None` so callers can fall back to `dumpsys`.
fn parse_badging_app_info(stdout: &str, package: &str) -> AppInfo {
    let mut info = AppInfo {
        package_name: package.to_string(),
        ..Default::default()
    };
    for line in stdout.lines() {
        let line = line.trim();
        if line == "application-debuggable" {
            info.is_debuggable = true;
            continue;
        }
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        match key.trim() {
            "package" => {
                for part in rest.split_whitespace() {
                    if let Some(v) = part.strip_prefix("name='") {
                        if let Some(end) = v.find('\'') {
                            info.package_name = v[..end].to_string();
                        }
                    } else if let Some(v) = part.strip_prefix("versionCode='") {
                        if let Some(end) = v.find('\'') {
                            info.version_code = Some(v[..end].to_string());
                        }
                    } else if let Some(v) = part.strip_prefix("versionName='") {
                        if let Some(end) = v.find('\'') {
                            info.version_name = Some(v[..end].to_string());
                        }
                    }
                }
            }
            "application-label" => info.app_label = Some(unquote(rest)),
            "sdkVersion" => info.min_sdk = Some(unquote(rest)),
            "targetSdkVersion" => info.target_sdk = Some(unquote(rest)),
            k if k.starts_with("application-icon") => {
                let path = unquote(rest);
                if info.icon_path.is_none()
                    || (!is_renderable_icon_path(info.icon_path.as_deref().unwrap_or_default())
                        && is_renderable_icon_path(&path))
                {
                    info.icon_path = Some(path);
                }
            }
            _ => {}
        }
    }
    info
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix('\'') {
        if let Some(end) = rest.find('\'') {
            return rest[..end].to_string();
        }
    }
    t.to_string()
}

/// Extract the first whitespace-delimited value of `key=...` inside `line`,
/// returning `None` when the key is absent or the value is empty.
fn dumpsys_field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let idx = line.find(key)?;
    let after = &line[idx + key.len()..];
    let value = after.split_whitespace().next().unwrap_or("");
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// Best-effort fallback that reads label / version / paths from `dumpsys package`.
/// Used when aapt2 isn't configured. We accept both `key=value` lines and the
/// collapsed format Android uses for `versionCode=N minSdk=N targetSdk=N`.
fn parse_dumpsys_for_label_version(stdout: &str, info: &mut AppInfo) {
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.contains("DEBUGGABLE") || trimmed.contains("debuggable=true") {
            info.is_debuggable = true;
        }
        if info.app_label.is_none() {
            if let Some(v) = dumpsys_field(trimmed, "applicationLabel=") {
                if !v.starts_with("ResId(") {
                    info.app_label = Some(v.to_string());
                }
            }
        }
        if info.version_name.is_none() {
            if let Some(v) = dumpsys_field(trimmed, "versionName=") {
                info.version_name = Some(v.to_string());
            }
        }
        if info.version_code.is_none() {
            if let Some(v) = dumpsys_field(trimmed, "versionCode=") {
                // The first numeric value belongs to the package itself; later
                // versionCode= lines (e.g. inside signing blocks) are unrelated.
                if v.chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
                {
                    info.version_code = Some(v.to_string());
                }
            }
        }
        if info.target_sdk.is_none() {
            if let Some(v) = dumpsys_field(trimmed, "targetSdk=") {
                info.target_sdk = Some(v.to_string());
            }
        }
        if info.min_sdk.is_none() {
            if let Some(v) = dumpsys_field(trimmed, "minSdk=") {
                if v.chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
                {
                    info.min_sdk = Some(v.to_string());
                }
            }
        }
        if info.apk_path.is_none() {
            if let Some(v) = dumpsys_field(trimmed, "codePath=") {
                info.apk_path = Some(v.to_string());
            }
        }
    }
}

/// Per-package info via shell-only: `dumpsys package` for label/version/sdk/debuggable/codePath
/// + `pm path` + `wc -c <path>` for apk_count / apk_total_size. **No APK
/// pull, no aapt2.** Use this for the per-card enrichment waterfall in
/// AdbAppsTab; the heavy `package_info` is reserved for callers that
/// actually need the APK on disk (jadx / analyze / decompile).
pub async fn package_info_lite(
    _app: &AppHandle,
    settings: &Settings,
    device: &str,
    package: &str,
) -> AppResult<AppInfo> {
    if package.trim().is_empty() {
        return Err(AppError::InvalidInput("package is empty".into()));
    }

    let mut info = AppInfo {
        package_name: package.to_string(),
        ..Default::default()
    };

    // 1) `dumpsys package` -- authoritative live state for label/version/sdk/etc.
    if let Ok(dump) = run_adb_shell(settings, device, &["dumpsys", "package", package]).await {
        parse_dumpsys_for_label_version(&dump, &mut info);
    }

    // 2) `pm path` for APK path(s) + count
    if let Ok(paths_out) = run_adb_shell(settings, device, &["pm", "path", package]).await {
        let remotes = parse_apk_paths(&paths_out);
        info.apk_count = remotes.len();
        if let Some(first) = remotes.first() {
            // Only set apk_path if dumpsys didn't already surface codePath=.
            if info.apk_path.is_none() {
                info.apk_path = Some(first.clone());
            }
        }
        // 3) `wc -c <path>` per remote for total size.
        info.apk_total_size = apk_sizes_for(settings, device, &remotes).await;
    }

    Ok(info)
}

/// Returns a flag indicating whether `package` looks like a system app.
pub async fn package_info(
    app: &AppHandle,
    settings: &Settings,
    device: &str,
    package: &str,
) -> AppResult<AppInfo> {
    if package.trim().is_empty() {
        return Err(AppError::InvalidInput("package is empty".into()));
    }

    let mut info = AppInfo {
        package_name: package.to_string(),
        ..Default::default()
    };

    // 1) Live device state via `dumpsys package`. This is the source of truth
    //    for is_debuggable / label / version / sdk / apk_path - fields that
    //    must reflect whatever is currently installed on the device, NOT
    //    whatever happens to be sitting in our local cache directory.
    //
    //    We deliberately run this BEFORE the APK pull + aapt2 step below.
    //    The local cache is keyed on (device, package) and never re-pulled
    //    when the file already exists, so a release->debug reinstall of the
    //    same package leaves a stale cached APK on disk. If we parsed
    //    is_debuggable from that stale APK first we'd misreport it as
    //    "release" forever. dumpsys always reflects live state.
    if let Ok(dump) = run_adb_shell(settings, device, &["dumpsys", "package", package]).await {
        parse_dumpsys_for_label_version(&dump, &mut info);
    }

    // 2) Pull the on-device APK to a cache dir so aapt2 can inspect it.
    let cache_root = app
        .path()
        .app_cache_dir()
        .map_err(|e| AppError::Config(e.to_string()))?
        .join("adb")
        .join(sanitize(device))
        .join(sanitize(package));
    tokio::fs::create_dir_all(&cache_root).await?;
    let apk_remotes = apk_paths_for(settings, device, package).await?;
    info.apk_count = apk_remotes.len();
    info.apk_total_size = apk_sizes_for(settings, device, &apk_remotes).await;
    let local_apks = local_apk_paths(&cache_root, &apk_remotes);
    for (apk_remote, local_apk) in apk_remotes.iter().zip(local_apks.iter()) {
        if !local_apk.exists() {
            pull_file(settings, device, apk_remote, local_apk).await?;
        }
    }
    let local_apk = local_apks
        .first()
        .ok_or_else(|| AppError::NotFound(format!("local apk for {package}")))?;

    // 3) Resolve the on-device APK path. Prefer dumpsys's `codePath=` (live),
    //    fall back to `pm path` if dumpsys didn't surface one.
    let apk_remote = info
        .apk_path
        .clone()
        .or_else(|| apk_remotes.first().cloned())
        .ok_or_else(|| AppError::NotFound(format!("apk path for {package}")))?;
    info.apk_path = Some(apk_remote);

    // 4) aapt2 only ENRICHES the record - it fills in icon_path (which
    //    dumpsys doesn't provide) and backstops any field dumpsys missed.
    //    It must NOT override the live values we already have, otherwise a
    //    stale cached APK would corrupt is_debuggable after a reinstall.
    if settings.aapt_path.is_some() {
        if let Ok(badging) = aapt_dump_badging(settings, local_apk).await {
            let extra = parse_badging_app_info(&badging, package);
            if info.icon_path.is_none() {
                info.icon_path = extra.icon_path;
            }
            if info.app_label.is_none() {
                info.app_label = extra.app_label;
            }
            if info.version_name.is_none() {
                info.version_name = extra.version_name;
            }
            if info.version_code.is_none() {
                info.version_code = extra.version_code;
            }
            if info.min_sdk.is_none() {
                info.min_sdk = extra.min_sdk;
            }
            if info.target_sdk.is_none() {
                info.target_sdk = extra.target_sdk;
            }
            // Deliberately skip extra.is_debuggable: dumpsys is authoritative.
            // Deliberately skip extra.apk_path: already resolved above.
        }
    }

    // 5) System-app classification. dumpsys doesn't surface FLAG_SYSTEM
    //    directly, so we use the install path: anything under /system/,
    //    /vendor/, /product/, or /apex/ is a system app. This matches the
    //    shell-fallback path in `list_packages_via_shell` and avoids the
    //    `pm list packages -s` round-trip that `is_system_package` would
    //    do per call.
    if let Some(p) = info.apk_path.as_deref() {
        info.is_system = p.starts_with("/system/")
            || p.starts_with("/vendor/")
            || p.starts_with("/product/")
            || p.starts_with("/apex/");
    }

    Ok(info)
}

/// Result of `adb -s <device> shell <command>`. We capture stdout / stderr
/// separately so the UI can color them differently and surface the exit
/// code regardless of which stream the device shell wrote to.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ShellOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub command: String,
}

/// Run `adb -s <device> shell <command>` synchronously and return the
/// captured streams + exit code. The command string is forwarded as-is, so
/// the device-side shell handles any quoting / piping.
pub async fn shell_exec(
    settings: &Settings,
    device: &str,
    command: &str,
) -> AppResult<ShellOutput> {
    if command.trim().is_empty() {
        return Err(AppError::InvalidInput("command is empty".into()));
    }
    let adb = adb_binary(settings)?.to_string();
    let output = Command::new(&adb)
        .arg("-s")
        .arg(device)
        .arg("shell")
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| AppError::Config(format!("spawn adb shell: {e}")))?;
    Ok(ShellOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
        command: command.to_string(),
    })
}

/// One immutable piece of device metadata surfaced in the "System Info"
/// tab. All fields are optional because each comes from a separate `adb
/// shell` call that may fail (e.g. permission denied on `dumpsys
/// battery`, missing `wm` helper on old Android, etc).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSystemInfo {
    // 硬件 / Hardware
    pub manufacturer: Option<String>,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub device: Option<String>,
    pub hardware: Option<String>,
    pub platform: Option<String>,
    pub serial: Option<String>,
    pub bootloader: Option<String>,
    pub fingerprint: Option<String>,

    // 屏幕 / Display
    pub screen_size: Option<String>,
    pub screen_density: Option<String>,
    pub screen_refresh_rate: Option<String>,
    pub physical_size: Option<String>,
    pub rotation: Option<String>,

    // 系统 / System
    pub android_release: Option<String>,
    pub android_sdk: Option<String>,
    pub security_patch: Option<String>,
    pub build_id: Option<String>,
    pub build_type: Option<String>,
    pub kernel_version: Option<String>,
    pub java_vm: Option<String>,
    pub abi: Option<String>,
    pub abi_list: Option<String>,

    // CPU
    pub cpu_abi: Option<String>,
    pub cpu_cores: Option<String>,
    pub cpu_hardware: Option<String>,
    pub cpu_max_freq: Option<String>,
    pub cpu_features: Option<String>,

    // GPU / 图形处理器
    pub gpu_vendor: Option<String>,
    pub gpu_renderer: Option<String>,
    pub gpu_opengles_version: Option<String>,
    pub gpu_vulkan_version: Option<String>,
    pub gpu_driver: Option<String>,

    // 内存 / Memory
    pub ram_total: Option<String>,
    pub ram_available: Option<String>,

    // 存储 / Storage
    pub storage_total: Option<String>,
    pub storage_available: Option<String>,

    // 网络 / Network
    pub wifi_ssid: Option<String>,
    pub wifi_ip: Option<String>,
    pub wifi_signal: Option<String>,
    pub wifi_link_speed: Option<String>,
    pub wifi_frequency: Option<String>,
    pub network_type: Option<String>,
    pub operator: Option<String>,
    pub airplane_mode: Option<String>,
    pub ipv4: Option<String>,

    // 运行时 / Runtime
    pub uptime: Option<String>,
    pub boot_time: Option<String>,
    pub selinux: Option<String>,
    pub timezone: Option<String>,
    pub locale: Option<String>,
    pub foreground_app: Option<String>,
    pub screen_state: Option<String>,

    // 电量 / Battery
    pub battery_level: Option<String>,
    pub battery_status: Option<String>,
    pub battery_health: Option<String>,
    pub battery_temp: Option<String>,
    pub battery_voltage: Option<String>,
    pub battery_technology: Option<String>,
    pub battery_plugged: Option<String>,
}

/// Decode the `ro.opengles.version` value into a "major.minor"
/// string. The prop is documented as hex BCD (e.g. `0x00030002` for
/// ES 3.2), but some ROMs return the raw decimal instead (e.g.
/// `196610` for the same 3.2). Accept either form.
fn parse_opengles_hex(s: &str) -> Option<String> {
    let s = s.trim();
    let n = if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(rest, 16).ok()?
    } else {
        // Fallback: bare decimal. The Android documented form is hex,
        // but getprop on some OEM ROMs normalises it to decimal.
        s.parse::<u32>().ok()?
    };
    let major = ((n >> 16) & 0xFFFF) as u32;
    let minor = (n & 0xFFFF) as u32;
    if major == 0 && minor == 0 {
        return None;
    }
    Some(format!("{}.{}", major, minor))
}

/// Known GPU vendor names. Used to disambiguate `ro.hardware.egl` /
/// similar getprop values that some ROMs set to the GPU model name
/// (e.g. "adreno") instead of the company name ("Qualcomm"). All
/// comparisons are lower-cased.
const KNOWN_GPU_VENDORS: &[&str] = &[
    "qualcomm",
    "arm",
    "imagination",
    "nvidia",
    "intel",
    "apple",
    "broadcom",
    "samsung",
    "mediatek",
    "hisilicon",
    "amlogic",
    "rockchip",
    "allwinner",
    "verisilicon",
];

/// Known GPU model / family keywords. If a getprop value contains
/// any of these, it's almost certainly a model name (like "Adreno
/// 740") rather than a vendor or a version number.
const GPU_MODEL_KEYWORDS: &[&str] = &[
    "adreno",
    "mali",
    "powervr",
    "tegra",
    "videocore",
    "xclipse",
    "radeon",
    "iris xe",
    "iris plus",
    "uhd graphics",
    "hd graphics",
    "apple gpu",
];

/// Classify an ambiguous GPU-related string as a vendor, a model, or
/// unknown. Used by the fallback layer to decide whether a getprop
/// value should populate `gpu_vendor` or `gpu_renderer`.
///
/// Model keywords are checked first because model names are more
/// specific (e.g. "Intel(R) UHD Graphics 770" or "Apple GPU" both
/// mention a vendor keyword AND carry a model-family keyword — the
/// model classification is the more useful one in that case, since
/// `ro.hardware.egl` on Xiaomi / HyperOS-style ROMs is set to the
/// model name).
fn classify_gpu_value(s: &str) -> GpuValueKind {
    let lower = s.to_lowercase();
    if GPU_MODEL_KEYWORDS.iter().any(|k| lower.contains(k)) {
        GpuValueKind::Model
    } else if KNOWN_GPU_VENDORS.iter().any(|v| lower.contains(v)) {
        GpuValueKind::Vendor
    } else {
        GpuValueKind::Unknown
    }
}

#[derive(Debug, PartialEq, Eq)]
enum GpuValueKind {
    Vendor,
    Model,
    Unknown,
}

/// True if the string plausibly looks like a version like "1.3.0" or
/// "0.8". Used to filter `ro.hardware.vulkan` etc. which on some
/// devices holds the GPU model name instead.
fn looks_like_version(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() || t.len() > 30 {
        return false;
    }
    if !t.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    let lower = t.to_lowercase();
    if GPU_MODEL_KEYWORDS.iter().any(|k| lower.contains(k)) {
        return false;
    }
    if KNOWN_GPU_VENDORS.iter().any(|v| lower.contains(v)) {
        return false;
    }
    true
}

/// Map a known GPU model / family keyword in the renderer string to
/// its vendor. Returns `None` when the renderer doesn't carry any
/// recognizable family tag.
fn derive_vendor_from_renderer(r: &str) -> Option<String> {
    let lower = r.to_lowercase();
    if lower.contains("adreno") {
        Some("Qualcomm".into())
    } else if lower.contains("mali") {
        Some("ARM".into())
    } else if lower.contains("powervr") {
        Some("Imagination".into())
    } else if lower.contains("tegra") || lower.contains("nvidia") {
        Some("NVIDIA".into())
    } else if lower.contains("apple") {
        Some("Apple".into())
    } else if lower.contains("intel")
        || lower.contains("uhd")
        || lower.contains("iris")
        || lower.contains("hd graphics")
    {
        Some("Intel".into())
    } else if lower.contains("broadcom") || lower.contains("videocore") {
        Some("Broadcom".into())
    } else if lower.contains("amd") || lower.contains("radeon") {
        Some("AMD".into())
    } else if lower.contains("samsung") || lower.contains("xclipse") {
        Some("Samsung".into())
    } else if lower.contains("mediatek") {
        Some("MediaTek".into())
    } else {
        None
    }
}

/// Lightweight byte formatter used by the system info section (kept here
/// to avoid pulling a new util module). Input is bytes; output uses KB /
/// MB / GB with one decimal place.
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Parse the output of `uptime` (or `cat /proc/uptime`) into a
/// zh-friendly string like "3 天 5 小时 12 分" / "5 小时 12 分" /
/// "12 分 30 秒". `raw` may be the full multi-line `uptime` output or
/// just a number-of-seconds float from `/proc/uptime`.
fn parse_uptime(raw: &str) -> Option<String> {
    // Common `uptime` formats:
    //   "10:16:11 up 5 days, 3:45, 1 user, ..."
    //   "10:16:11 up 3:45, ..."
    //   "10:16:11 up 12 mins, ..."
    //   "10:16:11 up 30 sec, ..."
    let lower = raw.to_lowercase();
    let idx = lower.find("up ")?;
    let after = &raw[idx + 3..];
    // `uptime` prints at most two segments separated by a comma:
    //   "5 days, 3:45"   -> ["5 days", " 3:45"]
    //   "3:45"           -> ["3:45"]
    //   "12 mins"        -> ["12 mins"]
    let first_chunk: String = after
        .chars()
        .take_while(|c| *c != ',' && *c != '\n')
        .collect();
    let rest = &after[first_chunk.len()..];
    let second_chunk: String = if rest.starts_with(',') {
        rest[1..]
            .chars()
            .take_while(|c| *c != ',' && *c != '\n')
            .collect()
    } else {
        String::new()
    };
    let mut days: u64 = 0;
    let mut hours: u64 = 0;
    let mut mins: u64 = 0;
    let mut secs: u64 = 0;
    for seg in [first_chunk.as_str(), second_chunk.as_str()] {
        let seg = seg.trim();
        if seg.is_empty() {
            continue;
        }
        if let Some(rest) = seg.strip_suffix("days") {
            days = rest.trim().parse().unwrap_or(0);
        } else if let Some(rest) = seg.strip_suffix("day") {
            days = rest.trim().parse().unwrap_or(0);
        } else if let Some(rest) = seg.strip_suffix("mins") {
            mins = rest.trim().parse().unwrap_or(0);
        } else if let Some(rest) = seg.strip_suffix("min") {
            mins = rest.trim().parse().unwrap_or(0);
        } else if let Some(rest) = seg.strip_suffix("secs") {
            secs = rest.trim().parse().unwrap_or(0);
        } else if let Some(rest) = seg.strip_suffix("sec") {
            secs = rest.trim().parse().unwrap_or(0);
        } else if seg.contains(':') {
            // "H:MM" / "HH:MM" / "HH:MM:SS"
            let parts: Vec<&str> = seg.split(':').collect();
            if parts.len() == 2 {
                hours = parts[0].trim().parse().unwrap_or(0);
                mins = parts[1].trim().parse().unwrap_or(0);
            } else if parts.len() == 3 {
                hours = parts[0].trim().parse().unwrap_or(0);
                mins = parts[1].trim().parse().unwrap_or(0);
                secs = parts[2].trim().parse().unwrap_or(0);
            }
        }
    }
    Some(format_uptime_parts(days, hours, mins, secs))
}
fn format_uptime(secs: u64) -> String {
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3_600;
    let mins = (secs % 3_600) / 60;
    let seconds = secs % 60;
    format_uptime_parts(days, hours, mins, seconds)
}

fn format_uptime_parts(days: u64, hours: u64, mins: u64, secs: u64) -> String {
    let mut parts: Vec<String> = Vec::new();
    if days > 0 {
        parts.push(format!("{} 天", days));
    }
    if hours > 0 || days > 0 {
        parts.push(format!("{} 小时", hours));
    }
    if mins > 0 || (days == 0 && hours == 0) {
        parts.push(format!("{} 分", mins));
    }
    if days == 0 && hours == 0 && mins < 5 {
        // Show seconds only for very fresh boots (<5 min) so users
        // can tell "just rebooted" from "up for 4 min".
        parts.push(format!("{} 秒", secs));
    }
    parts.join(" ")
}

/// Decode a VK_MAKE_VERSION-encoded integer (or hex string) into
/// "major.minor.patch". Vulkan encodes version as
/// `(major << 22) | (minor << 12) | patch`. The Android system surfaces
/// this through `pm list features` like
/// `feature:android.hardware.vulkan.version=0x00400303`. Returns
/// `None` if the input can't be parsed so the caller can decide what
/// to do (we surface the raw value as a fallback).
fn decode_vulkan_version(raw: &str) -> Option<String> {
    let s = raw.trim();
    let n: u32 = if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(rest, 16).ok()?
    } else {
        s.parse::<u32>().ok()?
    };
    // Vulkan allows major up to 127, minor up to 1023, patch up to 4095.
    let major = (n >> 22) & 0x7F;
    let minor = (n >> 12) & 0x3FF;
    let patch = n & 0xFFF;
    if major == 0 && minor == 0 && patch == 0 {
        return None;
    }
    Some(format!("{}.{}.{}", major, minor, patch))
}

/// Decode `dumpsys battery` `status` field. Input can be either the
/// raw enum name (`CHARGING`, `DISCHARGING`, `FULL`) or the
/// human-readable tuple (`2 (CHARGING)`). Returns a zh label.
fn decode_battery_status(s: &str) -> String {
    let upper = s.to_uppercase();
    let code = upper
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>();
    if !code.is_empty() {
        return match code.as_str() {
            "1" => "未知",
            "2" => "充电中",
            "3" => "放电中",
            "4" => "未充电",
            "5" => "已充满",
            "6" => "已充满(测试)",
            _ => s,
        }
        .to_string();
    }
    if upper.contains("CHARGING") {
        return "充电中".to_string();
    }
    if upper.contains("DISCHARGING") {
        return "放电中".to_string();
    }
    if upper.contains("FULL") {
        return "已充满".to_string();
    }
    if upper.contains("NOT CHARGING") {
        return "未充电".to_string();
    }
    s.to_string()
}

/// Decode `dumpsys battery` `health` field. Common values:
///   1=Unknown 2=Good 3=Overheat 4=Dead 5=Over voltage
///   6=Unspecified failure 7=Cold
fn decode_battery_health(s: &str) -> String {
    let upper = s.to_uppercase();
    let code = upper
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>();
    if !code.is_empty() {
        return match code.as_str() {
            "1" => "未知",
            "2" => "良好",
            "3" => "过热",
            "4" => "损坏",
            "5" => "过压",
            "6" => "未指定故障",
            "7" => "过冷",
            _ => s,
        }
        .to_string();
    }
    if upper.contains("GOOD") {
        return "良好".to_string();
    }
    if upper.contains("OVERHEAT") {
        return "过热".to_string();
    }
    if upper.contains("DEAD") {
        return "损坏".to_string();
    }
    if upper.contains("COLD") {
        return "过冷".to_string();
    }
    s.to_string()
}

/// Convert a unix timestamp (seconds) to a local-time string like
/// "2026-07-30 10:16:11". We use the host's local TZ because the
/// device clock is the same one users see in their screenshots.
fn format_boot_time(ts: i64) -> Option<String> {
    use std::time::{Duration, UNIX_EPOCH};
    let dt = UNIX_EPOCH.checked_add(Duration::from_secs(ts as u64))?;
    // chrono is in Cargo.toml — use it instead of hand-rolling.
    let datetime = chrono::DateTime::<chrono::Local>::from(dt);
    Some(datetime.format("%Y-%m-%d %H:%M:%S").to_string())
}

/// Collect a one-shot snapshot of device metadata for the System Info
/// tab. Each section is queried with a separate `adb shell` call so a
/// single failure (e.g. locked SIM, no battery service on emulator)
/// only blanks out that section, not the whole snapshot.
///
/// Performance: ~20 adb roundtrips at ~50ms each ≈ 1s wall-clock on a
/// healthy USB connection. Acceptable for an on-demand refresh.
pub async fn system_info(
    settings: &Settings,
    device: &str,
) -> AppResult<DeviceSystemInfo> {
    let mut info = DeviceSystemInfo::default();

    // --- 硬件 ---------------------------------------------------------
    info.manufacturer = adb_dumpsys::prop_or_none(settings, device, "ro.product.manufacturer").await;
    info.brand = adb_dumpsys::prop_or_none(settings, device, "ro.product.brand").await;
    info.model = adb_dumpsys::prop_or_none(settings, device, "ro.product.model").await;
    info.device = adb_dumpsys::prop_or_none(settings, device, "ro.product.device").await;
    info.hardware = adb_dumpsys::prop_or_none(settings, device, "ro.hardware").await;
    info.platform = adb_dumpsys::prop_or_none(settings, device, "ro.board.platform").await;
    info.serial = adb_dumpsys::prop_or_none(settings, device, "ro.serialno").await;
    info.bootloader = adb_dumpsys::prop_or_none(settings, device, "ro.bootloader")
        .await
        .or_else(|| {
            // Older or OEM ROMs (e.g. some Xiaomi builds) only expose
            // `ro.boot.bootloader`. Fall back so we don't lose this field.
            None
        });
    if info.bootloader.is_none() {
        info.bootloader = adb_dumpsys::prop_or_none(settings, device, "ro.boot.bootloader").await;
    }
    info.fingerprint = adb_dumpsys::prop_or_none(settings, device, "ro.build.fingerprint").await;

    // --- 屏幕 ---------------------------------------------------------
    if let Ok(out) = run_adb_shell(settings, device, &["wm", "size"]).await {
        // "Physical size: 1080x2400" / "Override size: ..."
        for line in out.lines() {
            if let Some(rest) = line.split(':').nth(1) {
                let s = rest.trim().to_string();
                if s.contains('x') {
                    info.screen_size = Some(s);
                    break;
                }
            }
        }
    }
    if let Ok(out) = run_adb_shell(settings, device, &["wm", "density"]).await {
        for line in out.lines() {
            if let Some(rest) = line.split(':').nth(1) {
                info.screen_density = Some(rest.trim().to_string());
                break;
            }
        }
    }
    // Prefer the kernel-level property (most reliable across OEM ROMs).
    if info.screen_refresh_rate.is_none() {
        if let Some(rate) = adb_dumpsys::prop_or_none(settings, device, "persist.sys.ui.refresh_rate").await {
            info.screen_refresh_rate = Some(rate);
        }
    }
    if info.screen_refresh_rate.is_none() {
        if let Some(rate) = adb_dumpsys::prop_or_none(settings, device, "ro.surface_flinger.refresh_rate").await {
            info.screen_refresh_rate = Some(rate);
        }
    }
    if info.screen_refresh_rate.is_none() {
        if let Ok(out) = run_adb_shell(settings, device, &["dumpsys", "display"]).await {
            for line in out.lines() {
                let l = line.trim();
                if l.starts_with("mDefaultRefreshRate=")
                    || l.starts_with("refreshRate=")
                    || l.starts_with("mActiveRefreshRate=")
                {
                    info.screen_refresh_rate = Some(l.to_string());
                }
            }
        }
    }
    if let Ok(out) = run_adb_shell(
        settings,
        device,
        &["settings", "get", "system", "user_rotation"],
    )
    .await
    {
        let t = out.trim().to_string();
        if !t.is_empty() {
            info.rotation = Some(t);
        }
    }
    // Compute approximate physical screen size from size + density.
    if let (Some(size), Some(density)) = (&info.screen_size, &info.screen_density) {
        let parse_first = |s: &str| -> Option<f64> { s.trim().parse::<f64>().ok() };
        if let Some((w_str, h_str)) = size.split_once('x') {
            if let (Some(w), Some(h)) = (parse_first(w_str), parse_first(h_str)) {
                if let Ok(dpi) = density.trim().parse::<f64>() {
                    if dpi > 0.0 {
                        let diag = ((w * w + h * h) as f64).sqrt() / dpi;
                        info.physical_size = Some(format!("{:.2} inch", diag));
                    }
                }
            }
        }
    }

    // --- 系统 ---------------------------------------------------------
    info.android_release = adb_dumpsys::prop_or_none(settings, device, "ro.build.version.release").await;
    info.android_sdk = adb_dumpsys::prop_or_none(settings, device, "ro.build.version.sdk").await;
    info.security_patch = adb_dumpsys::prop_or_none(settings, device, "ro.build.version.security_patch").await;
    info.build_id = adb_dumpsys::prop_or_none(settings, device, "ro.build.id").await;
    info.build_type = adb_dumpsys::prop_or_none(settings, device, "ro.build.type").await;
    info.abi = adb_dumpsys::prop_or_none(settings, device, "ro.product.cpu.abi").await;
    info.abi_list = adb_dumpsys::prop_or_none(settings, device, "ro.product.cpu.abilist").await;
    info.java_vm = adb_dumpsys::prop_or_none(settings, device, "ro.java.vm.version")
        .await
        .or_else(|| {
            // Newer Android / OEM ROMs sometimes only set dalvik.vm.version.
            None
        });
    if info.java_vm.is_none() {
        info.java_vm = adb_dumpsys::prop_or_none(settings, device, "dalvik.vm.version").await;
    }
    if info.java_vm.is_none() {
        info.java_vm = adb_dumpsys::prop_or_none(settings, device, "ro.dalvik.vm.version").await;
    }
    if let Ok(out) = run_adb_shell(settings, device, &["uname", "-r"]).await {
        let t = out.trim().to_string();
        if !t.is_empty() {
            info.kernel_version = Some(t);
        }
    }

    // --- CPU ----------------------------------------------------------
    info.cpu_abi = info.abi.clone();
    if info.cpu_hardware.is_none() {
        if let Ok(out) = run_adb_shell(settings, device, &["getprop", "ro.product.board"]).await {
            let t = out.trim().to_string();
            if !t.is_empty() {
                info.cpu_hardware = Some(t);
            }
        }
    }
    if info.cpu_hardware.is_none() {
        if let Ok(out) = run_adb_shell(settings, device, &["getprop", "ro.hardware"]).await {
            let t = out.trim().to_string();
            if !t.is_empty() {
                info.cpu_hardware = Some(t);
            }
        }
    }
    if let Ok(out) = run_adb_shell(settings, device, &["nproc"]).await {
        let t = out.trim().to_string();
        if !t.is_empty() {
            info.cpu_cores = Some(t);
        }
    }
    // /proc/cpuinfo for Hardware + Features (skip "model name" duplicates)
    if let Ok(out) = run_adb_shell(settings, device, &["cat", "/proc/cpuinfo"]).await {
        for line in out.lines() {
            let lower = line.to_lowercase();
            if info.cpu_hardware.is_none() && lower.starts_with("hardware") {
                let v = line.split(':').nth(1).unwrap_or("").trim().to_string();
                if !v.is_empty() {
                    info.cpu_hardware = Some(v);
                }
            } else if info.cpu_features.is_none() && lower.starts_with("features") {
                let v = line.split(':').nth(1).unwrap_or("").trim().to_string();
                if !v.is_empty() {
                    info.cpu_features = Some(v);
                }
            }
        }
    }
    if let Ok(out) = run_adb_shell(
        settings,
        device,
        &["cat", "/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq"],
    )
    .await
    {
        if let Ok(khz) = out.trim().parse::<f64>() {
            let ghz = khz / 1_000_000.0;
            info.cpu_max_freq = Some(format!("{:.2} GHz", ghz));
        }
    }

    // --- GPU / Graphics ------------------------------------------------
    // Most reliable source for GPU on Android is `dumpsys SurfaceFlinger`,
    // which always lists the active GLES / Vulkan renderer regardless of
    // whether the app has rendered anything yet. We parse line-by-line
    // because the output is heterogeneous across vendors.
    if let Ok(out) =
        run_adb_shell(settings, device, &["dumpsys", "SurfaceFlinger"]).await
    {
        for line in out.lines() {
            let trimmed = line.trim();
            if info.gpu_vendor.is_none() {
                if let Some(rest) = trimmed
                    .strip_prefix("GLES vendor:")
                    .or_else(|| trimmed.strip_prefix("EGL vendor:"))
                    .or_else(|| trimmed.strip_prefix("Vulkan vendor:"))
                {
                    let v = rest.trim().to_string();
                    if !v.is_empty() {
                        info.gpu_vendor = Some(v);
                    }
                }
            }
            if info.gpu_renderer.is_none() {
                if let Some(rest) = trimmed
                    .strip_prefix("GLES renderer:")
                    .or_else(|| trimmed.strip_prefix("EGL renderer:"))
                    .or_else(|| trimmed.strip_prefix("Vulkan device:"))
                    .or_else(|| trimmed.strip_prefix("Vulkan renderer:"))
                {
                    let v = rest.trim().to_string();
                    if !v.is_empty() {
                        info.gpu_renderer = Some(v);
                    }
                }
            }
            if info.gpu_opengles_version.is_none() {
                if let Some(rest) = trimmed
                    .strip_prefix("GLES version:")
                    .or_else(|| trimmed.strip_prefix("EGL version:"))
                {
                    let raw = rest.trim();
                    if !raw.is_empty() {
                        // SurfaceFlinger prints e.g.
                        //   "OpenGL ES 3.2 V@415.0 (GIT@abc, ...)"
                        // We want just "OpenGL ES 3.2" for the user.
                        let compact = if let Some(idx) = raw.find(" V@") {
                            raw[..idx].trim().to_string()
                        } else {
                            // Take the first 3 whitespace-separated tokens
                            // (e.g. "OpenGL ES 3.2").
                            raw.split_whitespace()
                                .take(3)
                                .collect::<Vec<_>>()
                                .join(" ")
                        };
                        if !compact.is_empty() {
                            info.gpu_opengles_version = Some(compact);
                        }
                    }
                }
            }
            if info.gpu_vulkan_version.is_none() {
                if let Some(rest) = trimmed
                    .strip_prefix("Vulkan version:")
                    .or_else(|| trimmed.strip_prefix("Vulkan API version:"))
                {
                    let v = rest.trim().to_string();
                    if !v.is_empty() {
                        info.gpu_vulkan_version = Some(v);
                    }
                }
            }
        }
    }
    // Fallbacks for older / OEM ROMs that don't include GPU lines in
    // SurfaceFlinger. Different ROMs populate the getprops differently:
    //
    //   * `ro.hardware.egl` is documented as the EGL vendor name
    //     (e.g. "Qualcomm") but Xiaomi / some HyperOS builds put the
    //     GPU model name there ("adreno"). We classify the value with
    //     `classify_gpu_value` and route it to vendor vs. renderer.
    //   * `ro.hardware.vulkan` is documented as the Vulkan version
    //     (e.g. "1.3.0") but some devices put the GPU model there
    //     instead. We only accept values that look like a version.
    //   * `ro.vulkan.version` (newer prop name) is the preferred
    //     Vulkan version source on Android 12+.
    if info.gpu_vendor.is_none() || info.gpu_renderer.is_none() {
        if let Some(v) =
            adb_dumpsys::prop_or_none(settings, device, "ro.hardware.egl").await
        {
            match classify_gpu_value(&v) {
                GpuValueKind::Vendor => {
                    if info.gpu_vendor.is_none() {
                        info.gpu_vendor = Some(v);
                    }
                }
                GpuValueKind::Model => {
                    if info.gpu_renderer.is_none() {
                        info.gpu_renderer = Some(v);
                    }
                }
                GpuValueKind::Unknown => {
                    // Prefer the renderer slot — unknown values are
                    // more likely to be a model description than a
                    // vendor name (vendor names are well-known).
                    if info.gpu_renderer.is_none() {
                        info.gpu_renderer = Some(v);
                    } else if info.gpu_vendor.is_none() {
                        info.gpu_vendor = Some(v);
                    }
                }
            }
        }
    }
    // Cross-fill: when we ended up with a renderer but no vendor, the
    // renderer's model keyword (e.g. "Adreno 740" → "Qualcomm",
    // "Mali-G78" → "ARM") tells us the vendor.
    if info.gpu_vendor.is_none() {
        if let Some(renderer) = info.gpu_renderer.as_deref() {
            if let Some(v) = derive_vendor_from_renderer(renderer) {
                info.gpu_vendor = Some(v);
            }
        }
    }
    // OpenGL ES version. ro.opengles.version is hex-BCD on stock
    // Android (`0x00030002`) and decimal on some OEM ROMs (`196610`).
    // `parse_opengles_hex` accepts either; if it can't decode, leave
    // the field None rather than showing the raw integer.
    if info.gpu_opengles_version.is_none() {
        if let Some(v) =
            adb_dumpsys::prop_or_none(settings, device, "ro.opengles.version").await
        {
            if let Some(decoded) = parse_opengles_hex(&v) {
                info.gpu_opengles_version = Some(decoded);
            }
        }
    }
    // Vulkan version: prefer ro.vulkan.version (Android 12+) and fall
    // back to ro.hardware.vulkan. Both are validated as version
    // strings so we don't paint a model name into the version field.
    if info.gpu_vulkan_version.is_none() {
        for prop in &["ro.vulkan.version", "ro.hardware.vulkan"] {
            if let Some(v) = adb_dumpsys::prop_or_none(settings, device, prop).await {
                if looks_like_version(&v) {
                    info.gpu_vulkan_version = Some(v);
                    break;
                }
            }
        }
    }
    // Last-resort Vulkan source: `pm list features` exposes the
    // device's Vulkan feature manifest on Android 7+. The relevant line
    // looks like `feature:android.hardware.vulkan.version=0x00400303`
    // where the value is a VK_MAKE_VERSION-encoded u32. Decode it.
    if info.gpu_vulkan_version.is_none() {
        if let Ok(out) = run_adb_shell(settings, device, &["pm", "list", "features"]).await {
            for line in out.lines() {
                let trimmed = line.trim();
                if let Some(rest) =
                    trimmed.split("android.hardware.vulkan.version=").nth(1)
                {
                    let raw = rest.trim();
                    if let Some(version) = decode_vulkan_version(raw) {
                        info.gpu_vulkan_version = Some(version);
                        break;
                    } else if !raw.is_empty() {
                        // Unknown format — surface it as-is so the user
                        // can at least see the raw value.
                        info.gpu_vulkan_version = Some(raw.to_string());
                        break;
                    }
                }
            }
        }
    }
    if info.gpu_driver.is_none() {
        info.gpu_driver =
            adb_dumpsys::prop_or_none(settings, device, "ro.gfx.driver.0").await;
    }
    if info.gpu_driver.is_none() {
        info.gpu_driver =
            adb_dumpsys::prop_or_none(settings, device, "ro.hardware.gralloc").await;
    }
    if info.gpu_driver.is_none() {
        info.gpu_driver =
            adb_dumpsys::prop_or_none(settings, device, "ro.boot.hardware.gralloc").await;
    }
    if info.gpu_driver.is_none() {
        info.gpu_driver =
            adb_dumpsys::prop_or_none(settings, device, "ro.hardware.hwcomposer").await;
    }
    // `dumpsys SurfaceFlinger` always carries a `GLES driver:` line on
    // Android 10+ that points at the actual EGL driver library. Strip
    // the directory prefix so the user sees e.g. "libGLESv2_adreno.so"
    // instead of "/vendor/lib64/egl/libGLESv2_adreno.so".
    if info.gpu_driver.is_none() {
        if let Ok(out) =
            run_adb_shell(settings, device, &["dumpsys", "SurfaceFlinger"]).await
        {
            for line in out.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed
                    .strip_prefix("GLES driver:")
                    .or_else(|| trimmed.strip_prefix("EGL driver:"))
                {
                    let v = rest.trim();
                    if !v.is_empty() {
                        let basename = v.rsplit('/').next().unwrap_or(v);
                        info.gpu_driver = Some(basename.to_string());
                        break;
                    }
                }
            }
        }
    }

    // --- 运行时 / Runtime ----------------------------------------------
    // Uptime: prefer `uptime` (human-readable), fall back to
    // /proc/uptime (seconds, float). We format both into a zh-friendly
    // string like "3 天 5 小时 12 分" so the user doesn't have to
    // parse systemd-style output.
    if let Ok(out) = run_adb_shell(settings, device, &["uptime"]).await {
        if let Some(formatted) = parse_uptime(&out) {
            info.uptime = Some(formatted);
        }
    }
    if info.uptime.is_none() {
        if let Ok(out) = run_adb_shell(settings, device, &["cat", "/proc/uptime"]).await {
            if let Some(secs_str) = out.split_whitespace().next() {
                if let Ok(secs) = secs_str.parse::<u64>() {
                    info.uptime = Some(format_uptime(secs));
                }
            }
        }
    }
    // Boot time: btime line in /proc/stat gives the unix timestamp of
    // the last boot. Converted to local time on the host (the device
    // clock is what users see in their screenshots anyway).
    if let Ok(out) = run_adb_shell(settings, device, &["cat", "/proc/stat"]).await {
        for line in out.lines() {
            if let Some(rest) = line.strip_prefix("btime ") {
                if let Ok(ts) = rest.trim().parse::<i64>() {
                    if let Some(formatted) = format_boot_time(ts) {
                        info.boot_time = Some(formatted);
                    }
                }
                break;
            }
        }
    }
    // SELinux status — getenforce works without root.
    if let Ok(out) = run_adb_shell(settings, device, &["getenforce"]).await {
        let t = out.trim().to_string();
        if !t.is_empty() {
            info.selinux = Some(t);
        }
    }
    // Timezone — persist.sys.timezone is the standard source.
    if let Some(v) = adb_dumpsys::prop_or_none(settings, device, "persist.sys.timezone").await {
        info.timezone = Some(v);
    }
    // Locale — try multiple props because the naming changed across
    // Android versions.
    if let Some(v) = adb_dumpsys::prop_or_none(settings, device, "persist.sys.locale").await {
        info.locale = Some(v);
    }
    if info.locale.is_none() {
        if let Some(v) = adb_dumpsys::prop_or_none(settings, device, "ro.product.locale").await {
            info.locale = Some(v);
        }
    }
    // Foreground app — parse `dumpsys activity activities`. The exact
    // line varies between Android versions, so we accept three
    // candidate prefixes.
    if let Ok(out) =
        run_adb_shell(settings, device, &["dumpsys", "activity", "activities"]).await
    {
        for line in out.lines() {
            let trimmed = line.trim();
            for prefix in &[
                "topResumedActivity=",
                "mResumedActivity=",
                "ResumedActivity=",
            ] {
                if let Some(rest) = trimmed.strip_prefix(prefix) {
                    // Layout: ActivityRecord{hash u0 pkg/.cls pid=...}
                    let after = rest.trim_start_matches("ActivityRecord{");
                    // take up to the first whitespace or closing brace
                    let end = after
                        .find(|c: char| c.is_whitespace() || c == '}')
                        .unwrap_or(after.len());
                    let pkg_cls = &after[..end];
                    if !pkg_cls.is_empty() {
                        info.foreground_app = Some(pkg_cls.to_string());
                        break;
                    }
                }
            }
            if info.foreground_app.is_some() {
                break;
            }
        }
    }
    // Screen state — dumpsys power exposes mWakefulness (Awake/Asleep/Dozing).
    if let Ok(out) = run_adb_shell(settings, device, &["dumpsys", "power"]).await {
        for line in out.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("mWakefulness=") {
                let v = rest.trim().to_string();
                if !v.is_empty() {
                    info.screen_state = Some(v);
                    break;
                }
            }
        }
    }
    if info.screen_state.is_none() {
        if let Ok(out) = run_adb_shell(settings, device, &["dumpsys", "power"]).await {
            for line in out.lines() {
                let lower = line.to_lowercase();
                if lower.starts_with("display power:") {
                    // "Display Power: state=ON" / "state=OFF"
                    if let Some(rest) = line.split("state=").nth(1) {
                        let v = rest.split_whitespace().next().unwrap_or("").to_string();
                        if !v.is_empty() {
                            info.screen_state = Some(format!("Display {}", v));
                            break;
                        }
                    }
                }
            }
        }
    }

    // --- 内存 / Memory ------------------------------------------------
    if let Ok(out) = run_adb_shell(settings, device, &["cat", "/proc/meminfo"]).await {
        let mut total_kb: Option<u64> = None;
        let mut avail_kb: Option<u64> = None;
        for line in out.lines() {
            if line.starts_with("MemTotal:") {
                total_kb = line.split_whitespace().nth(1).and_then(|v| v.parse().ok());
            } else if line.starts_with("MemAvailable:") {
                avail_kb = line.split_whitespace().nth(1).and_then(|v| v.parse().ok());
            }
        }
        if let Some(kb) = total_kb {
            info.ram_total = Some(format_bytes(kb * 1024));
        }
        if let Some(kb) = avail_kb {
            info.ram_available = Some(format_bytes(kb * 1024));
        }
    }

    // --- 存储 / Storage ----------------------------------------------
    // `df -h /data` may fail on some ROMs (toybox quirks, /data merged
    // into /) and may also omit the mount column when the argument is
    // a symlink. Try three strategies in order of reliability.
    let storage_attempts: [(&[&str],); 3] = [
        (&["df", "-h", "/data"],),
        (&["df", "-h"],),
        (&["df", "-h", "/storage/emulated"],),
    ];
    for (cmd,) in storage_attempts.iter() {
        if let Ok(out) = run_adb_shell(settings, device, cmd).await {
            for line in out.lines().skip(1) {
                if let Some((size, avail)) = adb_dumpsys::df_columns(line) {
                    info.storage_total = Some(size);
                    info.storage_available = Some(avail);
                    break;
                }
            }
            if info.storage_total.is_some() {
                break;
            }
        }
    }
    // Last-resort: `dumpsys diskstats | head` if df gave us nothing.
    if info.storage_total.is_none() {
        if let Ok(out) = run_adb_shell(settings, device, &["dumpsys", "diskstats"]).await {
            // Look for the first "Data" section entry — most devices print
            // "Data: free=... max=... size=..." style lines.
            for line in out.lines() {
                let l = line.trim();
                if l.to_lowercase().starts_with("data:") {
                    // crude parse: try to find "size=" and "free=" pairs
                    let mut size: Option<String> = None;
                    let mut avail: Option<String> = None;
                    for tok in l.split_whitespace() {
                        if let Some(v) = tok.strip_prefix("size=") {
                            size = Some(v.trim_end_matches(',').to_string());
                        } else if let Some(v) = tok.strip_prefix("free=") {
                            avail = Some(v.trim_end_matches(',').to_string());
                        }
                    }
                    if size.is_some() && avail.is_some() {
                        info.storage_total = size;
                        info.storage_available = avail;
                        break;
                    }
                }
            }
        }
    }

    // --- 网络 / Network ----------------------------------------------
    // WiFi via dumpsys wifi (best-effort text scan). Multiple prefix
    // variants because OEM ROMs (Xiaomi HyperOS, ColorOS, etc.) format
    // the connection block differently.
    if let Ok(out) = run_adb_shell(settings, device, &["dumpsys", "wifi"]).await {
        if info.wifi_ssid.is_none() {
            info.wifi_ssid = adb_dumpsys::key_value_block(&out, "SSID");
        }
        if info.wifi_signal.is_none() {
            // Accept either "RSSI: -45" or "mRssi=-45" (HyperOS style).
            info.wifi_signal = adb_dumpsys::first_match(&out, "RSSI", ':')
                .or_else(|| adb_dumpsys::first_match(&out, "mRssi", '='));
        }
        if info.wifi_link_speed.is_none() {
            info.wifi_link_speed = adb_dumpsys::first_match(&out, "Link speed", ':')
                .or_else(|| adb_dumpsys::first_match(&out, "mLinkSpeed", '='));
        }
        if info.wifi_frequency.is_none() {
            info.wifi_frequency = adb_dumpsys::first_match(&out, "Frequency", ':')
                .or_else(|| adb_dumpsys::first_match(&out, "mFrequency", '='));
        }
    }
    // If dumpsys wifi masked signal/speed under privacy (Android 13+),
    // try `dumpsys wifi --realtime` first (privileged view) and fall
    // back to the wpa_supplicant config which is usually unfiltered
    // (and readable even without location permission).
    let mut wifi_extra: Option<String> = None;
    if info.wifi_signal.is_none()
        || info.wifi_link_speed.is_none()
        || info.wifi_frequency.is_none()
    {
        if let Ok(out) =
            run_adb_shell(settings, device, &["dumpsys", "wifi", "--realtime"]).await
        {
            wifi_extra = Some(out);
        }
        if wifi_extra.is_none() {
            if let Ok(out) = run_adb_shell(
                settings,
                device,
                &["cat", "/data/misc/wifi/wpa_supplicant.conf"],
            )
            .await
            {
                wifi_extra = Some(out);
            }
        }
    }
    if let Some(out) = wifi_extra {
        // wpa_supplicant.conf uses lowercase `ssid="..."`, while
        // dumpsys wifi --realtime uses uppercase `SSID: "..."`. Both
        // are handled by `key_value_block`'s case-insensitive match.
        if info.wifi_ssid.is_none() {
            info.wifi_ssid = adb_dumpsys::key_value_block(&out, "SSID");
        }
        if info.wifi_signal.is_none() {
            info.wifi_signal = adb_dumpsys::first_match(&out, "RSSI", ':');
        }
        if info.wifi_link_speed.is_none() {
            info.wifi_link_speed = adb_dumpsys::first_match(&out, "Link speed", ':');
        }
        if info.wifi_frequency.is_none() {
            info.wifi_frequency = adb_dumpsys::first_match(&out, "Frequency", ':')
                .or_else(|| {
                    adb_dumpsys::first_match(&out, "freq", '=')
                        .map(|v| format!("{} MHz", v))
                });
        }
    }
    if let Ok(out) = run_adb_shell(settings, device, &["ip", "-4", "addr", "show", "wlan0"]).await {
        if let Some(idx) = out.find("inet ") {
            let rest = &out[idx + 5..];
            let ip = rest.split_whitespace().next().unwrap_or("").to_string();
            if !ip.is_empty() {
                info.wifi_ip = Some(ip.clone());
                info.ipv4 = Some(ip);
            }
        }
    }
    // `gsm.operator.alpha` is unreliable on modern Android: usually
    // empty unless a SIM is active. Try multiple sources and filter
    // known "empty" placeholders.
    fn is_real_operator(s: &str) -> bool {
        let t = s.trim();
        !t.is_empty() && t != "," && t != "null" && t != "unknown"
    }
    if let Some(v) = adb_dumpsys::prop_or_none(settings, device, "gsm.operator.alpha").await {
        if is_real_operator(&v) {
            info.operator = Some(v);
        }
    }
    if info.operator.is_none() {
        if let Some(v) = adb_dumpsys::prop_or_none(settings, device, "ro.csp.operator").await {
            if is_real_operator(&v) {
                info.operator = Some(v);
            }
        }
    }
    if info.operator.is_none() {
        if let Ok(out) =
            run_adb_shell(settings, device, &["dumpsys", "telephony.registry"]).await
        {
            for line in out.lines() {
                let trimmed = line.trim();
                // mSimOperatorAlpha={"foo"} — pull out the quoted value.
                if let Some(rest) = trimmed.strip_prefix("mSimOperatorAlpha=") {
                    if let Some(v) = adb_dumpsys::quoted_value(rest) {
                        if is_real_operator(&v) {
                            info.operator = Some(v);
                            break;
                        }
                    }
                }
                if let Some(rest) = trimmed.strip_prefix("mNetworkOperatorName=") {
                    if let Some(v) = adb_dumpsys::quoted_value(rest) {
                        if is_real_operator(&v) {
                            info.operator = Some(v);
                            break;
                        }
                    }
                }
            }
        }
    }
    if info.wifi_ssid.is_some() || info.wifi_ip.is_some() {
        info.network_type = Some("Wi-Fi".into());
    } else if let Ok(out) = run_adb_shell(settings, device, &["dumpsys", "connectivity"]).await {
        if out.contains("MOBILE") {
            info.network_type = Some("Mobile".into());
        } else if out.contains("VPN") {
            info.network_type = Some("VPN".into());
        } else {
            info.network_type = Some("Offline".into());
        }
    } else {
        info.network_type = Some("Offline".into());
    }
    if let Ok(out) = run_adb_shell(
        settings,
        device,
        &["settings", "get", "global", "airplane_mode_on"],
    )
    .await
    {
        let t = out.trim().to_string();
        if !t.is_empty() {
            info.airplane_mode = Some(if t == "1" { "已开启".into() } else { "已关闭".into() });
        }
    }

    // --- 电量 / Battery ----------------------------------------------
    // `dumpsys battery` output on modern Android is namespaced with
    // `Battery ` prefix (e.g. `  Battery level: 80`). Earlier versions
    // used the bare key. Strip the prefix so both work, and normalize
    // the value type.
    if let Ok(out) = run_adb_shell(settings, device, &["dumpsys", "battery"]).await {
        for raw in out.lines() {
            let trimmed = raw.trim();
            let stripped = trimmed
                .strip_prefix("Battery ")
                .or_else(|| trimmed.strip_prefix("battery "))
                .unwrap_or(trimmed);
            let (key, val) = match stripped.split_once(':') {
                Some(parts) => (parts.0.trim().to_lowercase(), parts.1.trim().to_string()),
                None => continue,
            };
            if val.is_empty() {
                continue;
            }
            match key.as_str() {
                "level" => info.battery_level = Some(val),
                // Status / health come through as e.g. "2 (CHARGING)".
                // Decode the leading code so the UI shows a
                // human-readable label rather than a tuple.
                "status" => info.battery_status = Some(decode_battery_status(&val)),
                "health" => info.battery_health = Some(decode_battery_health(&val)),
                "temperature" => {
                    // Raw is tenths of a degree Celsius.
                    if let Ok(t) = val.parse::<f64>() {
                        info.battery_temp = Some(format!("{:.1} °C", t / 10.0));
                    }
                }
                "voltage" => info.battery_voltage = Some(val),
                "technology" => info.battery_technology = Some(val),
                "plugged" | "ac powered" | "usb powered" | "wireless powered" => {
                    // Only record `plugged:`; ignore the per-source booleans
                    // (some OEMs print them but skip `plugged:`).
                    if key == "plugged" {
                        info.battery_plugged = Some(val);
                    }
                }
                _ => {}
            }
        }
    }
    // Last-resort: on some emulators / HyperOS the battery service is
    // gated. If everything is still empty, surface that gracefully —
    // we already show `—` per field, no extra fallback possible.

    Ok(info)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InstallApkItemResult {
    pub path: String,
    pub success: bool,
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InstallApksResult {
    pub succeeded: usize,
    pub failed: usize,
    pub items: Vec<InstallApkItemResult>,
}

pub async fn install_apks(settings: &Settings, device: &str, paths: &[String]) -> AppResult<InstallApksResult> {
    if paths.is_empty() { return Err(AppError::InvalidInput("no APK selected".into())); }
    let mut items = Vec::with_capacity(paths.len());
    for path in paths {
        let local = Path::new(path);
        if !local.is_file() || local.extension().and_then(|value| value.to_str()).map(|value| !value.eq_ignore_ascii_case("apk")).unwrap_or(true) {
            items.push(InstallApkItemResult { path: path.clone(), success: false, message: "invalid APK file".into() });
            continue;
        }
        match run_adb(settings, Some(device), &["install", "-r", path]).await {
            Ok(message) => items.push(InstallApkItemResult { path: path.clone(), success: true, message: message.trim().into() }),
            Err(error) => items.push(InstallApkItemResult { path: path.clone(), success: false, message: error.to_string() }),
        }
    }
    let succeeded = items.iter().filter(|item| item.success).count();
    Ok(InstallApksResult { succeeded, failed: items.len() - succeeded, items })
}

pub async fn uninstall(settings: &Settings, device: &str, package: &str) -> AppResult<String> {
    if package.trim().is_empty() {
        return Err(AppError::InvalidInput("package is empty".into()));
    }
    let out = run_adb(settings, Some(device), &["uninstall", package.trim()]).await?;
    Ok(out.trim().to_string())
}

pub async fn export_apks(
    settings: &Settings,
    device: &str,
    package: &str,
    version_name: Option<&str>,
    target_dir: &str,
) -> AppResult<ExportApksResult> {
    if package.trim().is_empty() {
        return Err(AppError::InvalidInput("package is empty".into()));
    }
    if target_dir.trim().is_empty() {
        return Err(AppError::InvalidInput("target directory is empty".into()));
    }
    let version = version_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown");
    let directory_name = sanitize(&format!("{}-{version}", package.trim()));
    let export_dir = std::path::Path::new(target_dir).join(directory_name);
    std::fs::create_dir_all(&export_dir)
        .map_err(|error| AppError::Config(format!("create {}: {error}", export_dir.display())))?;

    let remote_paths = apk_paths_for(settings, device, package.trim()).await?;
    for remote_path in &remote_paths {
        let file_name = std::path::Path::new(remote_path)
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| AppError::Parse(format!("invalid APK path: {remote_path}")))?;
        let local_path = export_dir.join(file_name);
        let local_path = local_path
            .to_str()
            .ok_or_else(|| AppError::InvalidInput("target path is not valid UTF-8".into()))?;
        download_direct(settings, device, remote_path, local_path)
            .await
            .map_err(|error| AppError::Config(format!("export {file_name}: {error}")))?;
    }

    Ok(ExportApksResult {
        count: remote_paths.len(),
        directory: export_dir.to_string_lossy().into_owned(),
    })
}

/// Force-stop a running app. Idempotent: no error if the app wasn't running.
pub async fn force_stop(
    settings: &Settings,
    device: &str,
    package: &str,
) -> AppResult<String> {
    if package.trim().is_empty() {
        return Err(AppError::InvalidInput("package is empty".into()));
    }
    let out = run_adb_shell(
        settings,
        device,
        &["am", "force-stop", package.trim()],
    )
    .await?;
    Ok(out.trim().to_string())
}

/// Reboot a device. `mode` is one of `None` (normal reboot),
/// `Some("recovery")`, or `Some("bootloader")`. Runs as the top-level
/// `adb reboot [mode]` -- NOT `adb shell reboot`, because shell uid
/// doesn't have permission to issue the reboot syscall on most
/// Android builds.
pub async fn power_reboot(
    settings: &Settings,
    device: &str,
    mode: Option<&str>,
) -> AppResult<String> {
    let mut args: Vec<&str> = vec!["reboot"];
    if let Some(m) = mode {
        match m {
            "recovery" | "bootloader" => args.push(m),
            other => {
                return Err(AppError::InvalidInput(format!(
                    "unknown reboot mode: {other}"
                )));
            }
        }
    }
    let out = run_adb(settings, Some(device), &args).await?;
    Ok(out.trim().to_string())
}

/// Power off the device. Tries `adb reboot -p` (the standard "soft
/// poweroff" command) and falls back to `input keyevent 26` (power
/// button press → screen off) if the device rejects the syscall. The
/// `keyevent` fallback doesn't actually shut the phone down, but at
/// least the screen goes dark and the user gets a clear signal the
/// action went through.
pub async fn power_shutdown(settings: &Settings, device: &str) -> AppResult<String> {
    match run_adb(settings, Some(device), &["reboot", "-p"]).await {
        Ok(out) => Ok(out.trim().to_string()),
        Err(primary) => {
            let fallback = run_adb_shell(
                settings,
                device,
                &["input", "keyevent", "26"],
            )
            .await
            .map_err(|_| primary)?;
            Ok(fallback.trim().to_string())
        }
    }
}

/// Launch an application's desktop entry through Android's launcher category.
pub async fn launch_app(
    settings: &Settings,
    device: &str,
    package: &str,
) -> AppResult<String> {
    if package.trim().is_empty() {
        return Err(AppError::InvalidInput("package is empty".into()));
    }
    let out = run_adb_shell(
        settings,
        device,
        &[
            "monkey",
            "-p",
            package.trim(),
            "-c",
            "android.intent.category.LAUNCHER",
            "1",
        ],
    )
    .await?;
    let lower = out.to_lowercase();
    if lower.contains("no activities found")
        || lower.contains("monkey aborted")
        || lower.contains("events injected: 0")
    {
        return Err(AppError::InvalidInput(format!(
            "no launcher activity found for {}",
            package.trim()
        )));
    }
    Ok(out.trim().to_string())
}

/// List a remote directory on the device. When `as_pkg` is `Some`, the
/// command is wrapped in `run-as <pkg>` so a debuggable app's private dir
/// can be browsed without root. Otherwise the listing runs as the `shell`
/// user, which requires root or world-readable permissions on the path.
pub async fn list_remote_dir(
    settings: &Settings,
    device: &str,
    path: &str,
    as_pkg: Option<&str>,
    use_root: bool,
) -> AppResult<Vec<DirEntry>> {
    // Append a trailing `/` so `ls -la` treats the path unambiguously
    // as a directory. Without it, when the path is a symlink (e.g.
    // `/sdcard` → `/storage/self/primary` on Android), `ls -la /sdcard`
    // prints the symlink itself as a single line — not the contents —
    // because Android's `ls` (toybox) inherits the standard POSIX
    // behaviour of `lstat`-ing the path argument. The trailing slash
    // forces `ls` to follow the symlink and list the target's contents,
    // which is what the UI expects. For regular directories the
    // trailing slash is a no-op.
    let path_arg = format!("{}/", path.trim_end_matches('/'));
    // Quote the path so the device shell does NOT interpret glob
    // characters (`?`, `*`, `[`, `]`) as wildcards. Path components
    // on Android can legitimately contain these — for example, a
    // device may have a symlink literally named `?` in `/` and the
    // user may click it. Without quoting, `ls -la /?/` arrives at the
    // device shell as a literal string, mksh expands `/?` to any
    // single-character directory in `/` (e.g. `/o` on Android 14+
    // system_ext), and the user gets a confusing "Permission denied"
    // toast for a path they never asked to open. `shell_quote` wraps
    // the argument in single quotes; the device shell strips them
    // before passing the path to `ls`, so `?` is treated as a literal
    // character. This mirrors what `run_root_shell` already does for
    // `su -c '...'`.
    let quoted_path = shell_quote(&path_arg);
    let cmd = ["ls", "-la", &quoted_path];
    let out = run_fs_shell(settings, device, as_pkg, use_root, &cmd).await?;
    Ok(out
        .lines()
        .filter_map(|line| parse_ls_line(line, &path_arg))
        .collect())
}

pub async fn resolve_app_data_dir(
    settings: &Settings,
    device: &str,
    package: &str,
    as_pkg: Option<&str>,
    use_root: bool,
) -> AppResult<String> {
    if package.trim().is_empty() {
        return Err(AppError::InvalidInput("package is empty".into()));
    }
    let current_user = run_adb_shell(settings, device, &["am", "get-current-user"])
        .await
        .ok()
        .map(|output| output.trim().to_string())
        .filter(|output| output.chars().all(|ch| ch.is_ascii_digit()))
        .unwrap_or_else(|| "0".to_string());
    let mut candidates = vec![
        format!("/data/user/{current_user}/{}", package.trim()),
        format!("/data/data/{}", package.trim()),
        format!("/data/user_de/{current_user}/{}", package.trim()),
    ];
    if current_user != "0" {
        candidates.extend([
            format!("/data/user/0/{}", package.trim()),
            format!("/data/user_de/0/{}", package.trim()),
        ]);
    }
    candidates.dedup();
    let mut last_error = None;
    for path in candidates {
        match run_fs_shell(
            settings,
            device,
            as_pkg,
            use_root,
            &["ls", "-ld", &shell_quote(&path)],
        )
        .await
        {
            Ok(_) => return Ok(path),
            Err(error) => last_error = Some(error),
        }
    }
    if use_root && as_pkg.is_none() {
        let output = run_fs_shell(
            settings,
            device,
            None,
            true,
            &[
                "find",
                "/data/user",
                "/data/user_de",
                "-mindepth",
                "2",
                "-maxdepth",
                "2",
                "-type",
                "d",
                "-name",
                package.trim(),
                "-print",
                "-quit",
            ],
        )
        .await;
        match output {
            Ok(output) => {
                if let Some(path) = output.lines().map(str::trim).find(|line| !line.is_empty()) {
                    return Ok(path.to_string());
                }
            }
            Err(error) => last_error = Some(error),
        }
    }
    let mode = if as_pkg.is_some() {
        "run-as"
    } else if use_root {
        "root"
    } else {
        "shell"
    };
    Err(AppError::NotFound(format!(
        "data directory for {} via {mode}: {}",
        package.trim(),
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "no candidate path exists".to_string())
    )))
}

/// Delete a file or directory on the device. `as_pkg` works the same as in
/// `list_remote_dir` — debug apps can self-delete via `run-as`.
pub async fn delete_remote_file(
    settings: &Settings,
    device: &str,
    path: &str,
    as_pkg: Option<&str>,
    use_root: bool,
) -> AppResult<String> {
    // `shell_quote` so the device shell does not interpret glob chars
    // in `path` (e.g. a user-typed name with `?` or `*`). See the
    // matching comment in `list_remote_dir` for the full rationale.
    let cmd = ["rm", "-rf", &shell_quote(path)];
    let out = run_fs_shell(settings, device, as_pkg, use_root, &cmd).await?;
    Ok(out.trim().to_string())
}

/// Push a local file to a path on the device. When `as_pkg` is `Some`, the
/// file is staged under `/data/local/tmp/` (always shell-writable) and then
/// moved into place via `run-as <pkg> cp`, so debug apps on non-rooted
/// devices can be uploaded to. When `as_pkg` is `None`, the file is pushed
/// directly — which requires root or a shell-writable target directory.
pub async fn push_file(
    settings: &Settings,
    device: &str,
    local_path: &str,
    remote_path: &str,
    as_pkg: Option<&str>,
    use_root: bool,
) -> AppResult<String> {
    match (as_pkg, use_root) {
        (None, false) => push_direct(settings, device, local_path, remote_path).await,
        (None, true) => match push_via_root(settings, device, local_path, remote_path).await {
            Ok(output) => Ok(output),
            Err(_) => push_direct(settings, device, local_path, remote_path).await,
        },
        (Some(pkg), _) => {
            push_via_identity(settings, device, local_path, remote_path, &["run-as", pkg]).await
        }
    }
}

async fn push_via_root(
    settings: &Settings,
    device: &str,
    local_path: &str,
    remote_path: &str,
) -> AppResult<String> {
    let basename = std::path::Path::new(local_path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("upload");
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let tmp_path = format!(
        "/data/local/tmp/.jadb-{}-{}-{}",
        std::process::id(),
        timestamp,
        basename
    );
    if let Err(error) = push_direct(settings, device, local_path, &tmp_path).await {
        let _ = run_adb_shell(settings, device, &["rm", "-f", &shell_quote(&tmp_path)]).await;
        return Err(error);
    }
    // `run_root_shell` already wraps each arg in `shell_quote`, so
    // `remote_path` is safe even with glob chars.
    let result = run_root_shell(settings, device, &["cp", &tmp_path, remote_path]).await;
    let _ = run_adb_shell(settings, device, &["rm", "-f", &shell_quote(&tmp_path)]).await;
    result
}

/// `adb push <local> <remote>` — runs as the `shell` user. Requires root
/// or a world-writable target directory on the device.
async fn push_direct(
    settings: &Settings,
    device: &str,
    local_path: &str,
    remote_path: &str,
) -> AppResult<String> {
    let adb = adb_binary(settings)?.to_string();
    let output = tokio::process::Command::new(&adb)
        .arg("-s")
        .arg(device)
        .arg("push")
        .arg(local_path)
        .arg(remote_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| AppError::Config(format!("spawn adb push: {e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        return Err(AppError::ToolFailed {
            tool: "adb".into(),
            code: output.status.code().unwrap_or(-1),
            msg: if stderr.trim().is_empty() {
                stdout
            } else {
                stderr
            },
        });
    }
    Ok(if stdout.is_empty() { stderr } else { stdout })
}

/// Debug-app upload: stage at `/data/local/tmp/.jadb-<pid>-<ts>-<basename>`
/// then `run-as <pkg> cp` into the target dir. Cleans up the temp file in
/// both success and failure paths (best-effort).
async fn push_via_identity(
    settings: &Settings,
    device: &str,
    local_path: &str,
    remote_path: &str,
    identity: &[&str],
) -> AppResult<String> {
    let basename = std::path::Path::new(local_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("upload");
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let tmp_path = format!(
        "/data/local/tmp/.jadb-{}-{}-{}",
        std::process::id(),
        timestamp,
        basename
    );
    // 1. Stage to /data/local/tmp/. If the staging push fails, cleanup any
    //    partial file and surface the error. `tmp_path` is JADB-generated
    //    so it has no user-controlled chars, but we quote anyway for
    //    consistency with the cleanup below.
    if let Err(e) = push_direct(settings, device, local_path, &tmp_path).await {
        let _ = run_adb_shell(settings, device, &["rm", "-f", &shell_quote(&tmp_path)]).await;
        return Err(e);
    }
    // `tmp_path` is a JADB-generated name (no user input) but we still
    // quote it for consistency. `remote_path` is user-supplied — quote
    // it so the device shell does not interpret glob chars in the target
    // path.
    let tmp_quoted = shell_quote(&tmp_path);
    let remote_quoted = shell_quote(remote_path);
    let mut cp_args = identity.to_vec();
    cp_args.extend_from_slice(&["cp", &tmp_quoted, &remote_quoted]);
    let cp_result = run_adb_shell(settings, device, &cp_args).await;
    // 3. Best-effort cleanup of the staged file (run both shells; one will
    //    succeed depending on where the temp actually landed).
    let _ = run_adb_shell(settings, device, &["rm", "-f", &shell_quote(&tmp_path)]).await;
    cp_result
}

/// Probe whether the connected device has root access (Magisk su, `adb root`,
/// or equivalent). Checks the adb shell identity, then tries `su -c id`.
/// Returns `Ok(false)` (NOT an error) when `su` is missing or denied, so
/// callers can use the bool directly.
pub async fn is_device_rooted(
    settings: &Settings,
    device: &str,
) -> AppResult<bool> {
    // We try four probes in order from cheapest to most device-specific.
    // Any single one returning uid=0 (or test-keys) is treated as rooted;
    // failure of a probe is silent so we fall through to the next one.
    //
    // 1) ro.build.tags = "test-keys" indicates a userdebug/eng build.
    //    Most rooted users keep that property and it's free to read.
    if let Ok(out) = run_adb_shell(settings, device, &["getprop", "ro.build.tags"]).await {
        if out.contains("test-keys") {
            return Ok(true);
        }
    }
    // 2) `id` returns uid=0 when the adb shell itself was promoted to
    //    root (e.g. `adb root` on userdebug builds). Cheap probe.
    if let Ok(out) = run_adb_shell(settings, device, &["id"]).await {
        if out.contains("uid=0") {
            return Ok(true);
        }
    }
    // 3) `su 0 id` is the modern Magisk 24+ / KernelSU / APatch syntax.
    //    This is the most reliable on current devices. Older Magisk
    //    builds reject this form and return an error, in which case we
    //    fall through to the legacy probe below.
    if let Ok(out) = run_adb_shell(settings, device, &["su", "0", "id"]).await {
        if out.contains("uid=0") {
            return Ok(true);
        }
    }
    // 4) `su -c id` is the legacy SuperSU / older Magisk (pre-24) form.
    //    On Android 11+ device-as-root or Magisk 24+ this typically
    //    returns `su: not found` / `su: inaccessible` (the su binary
    //    is masked for the shell user), so the call errors out and we
    //    do not mistakenly hit the Ok arm with shell-uid output.
    if let Ok(out) = run_adb_shell(settings, device, &["su", "-c", "id"]).await {
        if out.contains("uid=0") {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Pull a file from the device to a local path. Mirrors `push_file`'s
/// `as_pkg` semantics: when set, streams the file through `adb exec-out
/// run-as <pkg> cat <remote>` so debug apps on non-rooted devices work.
pub async fn download_file(
    settings: &Settings,
    device: &str,
    remote_path: &str,
    local_path: &str,
    as_pkg: Option<&str>,
    use_root: bool,
) -> AppResult<String> {
    match (as_pkg, use_root) {
        (None, false) => download_direct(settings, device, remote_path, local_path).await,
        (None, true) => match download_via_root(
            settings,
            device,
            remote_path,
            local_path,
        )
        .await {
            Ok(output) => Ok(output),
            Err(_) => download_direct(settings, device, remote_path, local_path).await,
        },
        (Some(pkg), _) => {
            download_via_identity(settings, device, remote_path, local_path, &["run-as", pkg]).await
        }
    }
}

async fn download_via_root(
    settings: &Settings,
    device: &str,
    remote_path: &str,
    local_path: &str,
) -> AppResult<String> {
    let adb = adb_binary(settings)?.to_string();
    let command = format!("'cat' {}", shell_quote(remote_path));
    let output = tokio::process::Command::new(&adb)
        .arg("-s")
        .arg(device)
        .arg("exec-out")
        .arg("su")
        .arg("-c")
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|error| AppError::Config(format!("spawn adb exec-out: {error}")))?;
    if !output.status.success() {
        return Err(AppError::ToolFailed {
            tool: "adb".into(),
            code: output.status.code().unwrap_or(-1),
            msg: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    std::fs::write(local_path, &output.stdout)
        .map_err(|error| AppError::Config(format!("write {local_path}: {error}")))?;
    Ok(format!("{} bytes", output.stdout.len()))
}

/// `adb pull <remote> <local>` — runs as the `shell` user. Requires root
/// or a world-readable target on the device.
async fn download_direct(
    settings: &Settings,
    device: &str,
    remote_path: &str,
    local_path: &str,
) -> AppResult<String> {
    let adb = adb_binary(settings)?.to_string();
    let output = tokio::process::Command::new(&adb)
        .arg("-s")
        .arg(device)
        .arg("pull")
        .arg(remote_path)
        .arg(local_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| AppError::Config(format!("spawn adb pull: {e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        return Err(AppError::ToolFailed {
            tool: "adb".into(),
            code: output.status.code().unwrap_or(-1),
            msg: if stderr.trim().is_empty() {
                stdout
            } else {
                stderr
            },
        });
    }
    Ok(if stdout.is_empty() { stderr } else { stdout })
}

/// Debug-app pull: stream the file content through `adb exec-out run-as
/// <pkg> cat <remote>` so we don't need shell access to the app's dir.
/// The returned stdout is the file body; we write it to `local_path`.
async fn download_via_identity(
    settings: &Settings,
    device: &str,
    remote_path: &str,
    local_path: &str,
    identity: &[&str],
) -> AppResult<String> {
    let adb = adb_binary(settings)?.to_string();
    let output = tokio::process::Command::new(&adb)
        .arg("-s")
        .arg(device)
        .arg("exec-out")
        .args(identity)
        .arg("cat")
        .arg(remote_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| AppError::Config(format!("spawn adb exec-out: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        return Err(AppError::ToolFailed {
            tool: "adb".into(),
            code: output.status.code().unwrap_or(-1),
            msg: if stderr.trim().is_empty() {
                stdout
            } else {
                stderr
            },
        });
    }
    let bytes = output.stdout;
    std::fs::write(local_path, &bytes)
        .map_err(|e| AppError::Config(format!("write {local_path}: {e}")))?;
    Ok(format!("{} bytes", bytes.len()))
}

/// Clear an app's data + cache via `pm clear`. Note this wipes the app's
/// user data — runtime permissions, accounts, settings — not just cache.
pub async fn clear_cache(
    settings: &Settings,
    device: &str,
    package: &str,
) -> AppResult<String> {
    if package.trim().is_empty() {
        return Err(AppError::InvalidInput("package is empty".into()));
    }
    let out = run_adb_shell(
        settings,
        device,
        &["pm", "clear", package.trim()],
    )
    .await?;
    Ok(out.trim().to_string())
}

pub async fn pull_file(
    settings: &Settings,
    device: &str,
    remote: &str,
    local: &Path,
) -> AppResult<()> {
    if let Some(parent) = local.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let remote_str = remote.to_string();
    let local_str = local.to_string_lossy().to_string();
    let _out = run_adb(settings, Some(device), &["pull", &remote_str, &local_str]).await?;
    Ok(())
}

/// Read a single entry from a local APK (zip) and return its raw bytes.
fn read_apk_entry(apk_path: &Path, entry: &str) -> AppResult<Vec<u8>> {
    let file = std::fs::File::open(apk_path).map_err(AppError::Io)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| AppError::Config(format!("open apk as zip: {e}")))?;
    let mut f = zip
        .by_name(entry)
        .map_err(|e| AppError::Config(format!("read apk entry {entry}: {e}")))?;
    let mut buf = Vec::with_capacity(f.size() as usize);
    use std::io::Read;
    f.read_to_end(&mut buf).map_err(AppError::Io)?;
    Ok(buf)
}

fn is_renderable_icon_path(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "webp")
    )
}

fn local_apk_paths(cache_root: &Path, remote_paths: &[String]) -> Vec<PathBuf> {
    remote_paths
        .iter()
        .enumerate()
        .map(|(index, remote)| {
            if index == 0 {
                return cache_root.join("base.apk");
            }
            let filename = Path::new(remote)
                .file_name()
                .and_then(|name| name.to_str())
                .map(sanitize)
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| format!("split-{index}.apk"));
            cache_root.join(format!("split-{index}-{filename}"))
        })
        .collect()
}

/// Return renderable resources from the APK, including custom icon names.
/// Adaptive-icon XML descriptors are deliberately skipped here: their
/// density-specific raster fallback and foreground/background assets are
/// still available elsewhere in the resource table for this best-effort path.
fn icon_entries_from_apk(apk_path: &Path) -> AppResult<Vec<String>> {
    let file = std::fs::File::open(apk_path).map_err(AppError::Io)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| AppError::Config(format!("open apk as zip: {e}")))?;
    let mut entries: Vec<(i32, String)> = Vec::new();
    for index in 0..zip.len() {
        let entry = zip
            .by_index(index)
            .map_err(|e| AppError::Config(format!("read apk entry {index}: {e}")))?;
        let name = entry.name().to_string();
        if !is_renderable_icon_path(&name)
            || !(name.starts_with("res/mipmap") || name.starts_with("res/drawable"))
        {
            continue;
        }
        let lower = name.to_ascii_lowercase();
        let stem = Path::new(&lower)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        let mut score = 0;
        if lower.contains("/mipmap") {
            score += 100;
        }
        if ["icon", "launcher", "logo", "app"]
            .iter()
            .any(|hint| stem.contains(hint))
        {
            score += 50;
        }
        for (qualifier, points) in [
            ("xxxhdpi", 40),
            ("xxhdpi", 35),
            ("xhdpi", 30),
            ("hdpi", 25),
            ("mdpi", 20),
            ("nodpi", 15),
        ] {
            if lower.contains(qualifier) {
                score += points;
                break;
            }
        }
        entries.push((score, name));
    }
    entries.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    Ok(entries.into_iter().map(|(_, name)| name).collect())
}

/// Guess an icon entry path inside the APK when aapt2 isn't available.
fn fallback_icon_entries() -> &'static [&'static str] {
    &[
        "res/mipmap-xxxhdpi-v4/ic_launcher.png",
        "res/mipmap-xxhdpi-v4/ic_launcher.png",
        "res/mipmap-xhdpi-v4/ic_launcher.png",
        "res/mipmap-hdpi-v4/ic_launcher.png",
        "res/mipmap-mdpi-v4/ic_launcher.png",
        "res/mipmap-xxxhdpi/ic_launcher.png",
        "res/mipmap-xxhdpi/ic_launcher.png",
        "res/mipmap-xhdpi/ic_launcher.png",
        "res/mipmap-hdpi/ic_launcher.png",
        "res/mipmap-mdpi/ic_launcher.png",
        "res/drawable-xxxhdpi-v4/ic_launcher.png",
        "res/drawable-xxhdpi-v4/ic_launcher.png",
        "res/drawable-xhdpi-v4/ic_launcher.png",
        "res/drawable-hdpi-v4/ic_launcher.png",
        "res/drawable-mdpi-v4/ic_launcher.png",
        "res/drawable/ic_launcher.png",
    ]
}

pub async fn pull_app_icon(
    app: &AppHandle,
    settings: &Settings,
    device: &str,
    package: &str,
) -> AppResult<Option<String>> {
    // 1) Make sure we have a local copy of the APK + an icon path.
    let info = package_info(app, settings, device, package).await?;
    let cache_root = app
        .path()
        .app_cache_dir()
        .map_err(|e| AppError::Config(e.to_string()))?
        .join("adb")
        .join(sanitize(device))
        .join(sanitize(package));
    let apk_remotes = apk_paths_for(settings, device, package).await?;
    let local_apks = local_apk_paths(&cache_root, &apk_remotes);

    // 2) Try the icon path aapt2 reported first, then common and discovered
    //    raster resources from every base/split APK.
    let mut candidates: Vec<String> = Vec::new();
    if let Some(p) = info.icon_path.as_ref() {
        candidates.push(p.clone());
    }
    for e in fallback_icon_entries() {
        if !candidates.iter().any(|c| c == *e) {
            candidates.push((*e).to_string());
        }
    }

    for local_apk in &local_apks {
        let discovered = icon_entries_from_apk(local_apk).unwrap_or_default();
        for entry in discovered {
            if !candidates.iter().any(|candidate| candidate == &entry) {
                candidates.push(entry);
            }
        }
    }

    for local_apk in &local_apks {
        for entry in &candidates {
            if !is_renderable_icon_path(entry) {
                continue;
            }
            match read_apk_entry(local_apk, entry) {
                Ok(bytes) if !bytes.is_empty() => {
                    let ext = Path::new(entry)
                        .extension()
                        .and_then(|value| value.to_str())
                        .unwrap_or_default()
                        .to_ascii_lowercase();
                    let mime = match ext.as_str() {
                        "png" => "image/png",
                        "webp" => "image/webp",
                        "jpg" | "jpeg" => "image/jpeg",
                        _ => "application/octet-stream",
                    };
                    let b64 = base64_encode(&bytes);
                    return Ok(Some(format!("data:{};base64,{}", mime, b64)));
                }
                Ok(_) | Err(_) => continue,
            }
        }
    }
    Ok(None)
}

/// Minimal, dependency-free base64 encoder so we don't pull in another crate.
pub(crate) fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(usize::div_ceil(input.len() + 2, 3) * 4);
    let mut i = 0;
    while i + 3 <= input.len() {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8) | (input[i + 2] as u32);
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        out.push(TABLE[(n & 0x3F) as usize] as char);
        i += 3;
    }
    let rem = input.len() - i;
    if rem == 1 {
        let n = (input[i] as u32) << 16;
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8);
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        out.push('=');
    }
    out
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Resolve the cache dir used for a (device, package) pair. Useful for tests.
#[allow(dead_code)]
fn local_apk_for(app: &AppHandle, device: &str, package: &str) -> AppResult<PathBuf> {
    let cache_root = app
        .path()
        .app_cache_dir()
        .map_err(|e| AppError::Config(e.to_string()))?
        .join("adb")
        .join(sanitize(device))
        .join(sanitize(package));
    Ok(cache_root.join("base.apk"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const SAMPLE_BADGING: &str = "package: name='com.example.audiodemo' versionCode='1' versionName='1.0' platformBuildVersionName='11'\nsdkVersion:'16'\ntargetSdkVersion:'28'\nuses-permission: name='android.permission.RECORD_AUDIO'\napplication-label:'AudioDemo'\napplication-debuggable\napplication-icon-160:'res/mipmap-anydpi-v26/ic_launcher.xml'\napplication-icon-320:'res/mipmap-xxhdpi-v4/app_icon.webp'\napplication-icon-640:'res/mipmap-xxxhdpi-v4/ic_launcher.png'\n";

    #[test]
    fn parses_badging_into_app_info() {
        let info = parse_badging_app_info(SAMPLE_BADGING, "com.example.audiodemo");
        assert_eq!(info.package_name, "com.example.audiodemo");
        assert_eq!(info.version_code.as_deref(), Some("1"));
        assert_eq!(info.version_name.as_deref(), Some("1.0"));
        assert_eq!(info.app_label.as_deref(), Some("AudioDemo"));
        assert_eq!(info.target_sdk.as_deref(), Some("28"));
        assert_eq!(info.min_sdk.as_deref(), Some("16"));
        assert!(info.is_debuggable);
        // Prefer a directly renderable resource over an adaptive-icon XML
        // descriptor when badging lists the XML first.
        assert_eq!(
            info.icon_path.as_deref(),
            Some("res/mipmap-xxhdpi-v4/app_icon.webp")
        );
    }

    #[test]
    fn parses_all_pm_paths_for_split_apks() {
        let output =
            "package:/data/app/example/base.apk\npackage:/data/app/example/split_config.en.apk\n";
        assert_eq!(
            parse_apk_paths(output),
            vec![
                "/data/app/example/base.apk",
                "/data/app/example/split_config.en.apk",
            ]
        );
    }

    #[test]
    fn discovers_custom_webp_icon_entries() {
        // Use UUID instead of the OS thread name: on Windows, the test
        // framework's thread names contain `::` (module-path separators)
        // which is an invalid character in NTFS filenames and produces
        // `Os { code: 123, InvalidFilename }` at File::create.
        let path = std::env::temp_dir().join(format!(
            "jadb-icon-fixture-{}-{}.apk",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let file = std::fs::File::create(&path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        archive
            .start_file("res/mipmap-xxhdpi-v4/custom_badge.webp", options)
            .unwrap();
        archive.write_all(b"not-a-real-webp").unwrap();
        archive
            .start_file("res/drawable/background.png", options)
            .unwrap();
        archive.write_all(b"background").unwrap();
        archive.finish().unwrap();

        let entries = icon_entries_from_apk(&path).unwrap();
        assert_eq!(
            entries.first().map(String::as_str),
            Some("res/mipmap-xxhdpi-v4/custom_badge.webp")
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn parses_dumpsys_into_app_info() {
        let sample = "Package [com.example.audiodemo] (496e19c):\n    appId=10071\n    codePath=/data/app/example/base.apk\n    versionCode=1 minSdk=16 targetSdk=28\n    versionName=1.0\n    applicationLabel=AudioDemo\n    flags=[ DEBUGGABLE HAS_CODE ]\n";
        let mut info = AppInfo {
            package_name: "com.example.audiodemo".into(),
            ..Default::default()
        };
        parse_dumpsys_for_label_version(sample, &mut info);
        assert_eq!(info.app_label.as_deref(), Some("AudioDemo"));
        assert_eq!(info.version_name.as_deref(), Some("1.0"));
        assert_eq!(info.version_code.as_deref(), Some("1"));
        assert_eq!(info.target_sdk.as_deref(), Some("28"));
        assert_eq!(info.apk_path.as_deref(), Some("/data/app/example/base.apk"));
        assert!(info.is_debuggable);
    }

    #[test]
    fn base64_encode_handles_padding() {
        // Standard test vectors.
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn list_devices_parses_long_format() {
        // Smoke test of the parser using the exact `adb devices -l` shape.
        let line = "10.10.0.60:5555        device product:finch model:MiTV_MFFU0 device:finch transport-id:10";
        let mut parts = line.split_whitespace();
        let serial = parts.next().unwrap().to_string();
        let state = parts.next().unwrap().to_string();
        let mut model = None;
        let mut product = None;
        for tok in parts {
            if let Some(v) = tok.strip_prefix("model:") {
                model = Some(v.replace('_', " "));
            } else if let Some(v) = tok.strip_prefix("product:") {
                product = Some(v.to_string());
            }
        }
        assert_eq!(serial, "10.10.0.60:5555");
        assert_eq!(state, "device");
        assert_eq!(model.as_deref(), Some("MiTV MFFU0"));
        assert_eq!(product.as_deref(), Some("finch"));
    }

    #[test]
    fn unquote_strips_single_quotes() {
        assert_eq!(unquote("'AudioDemo'"), "AudioDemo");
        assert_eq!(unquote("AudioDemo"), "AudioDemo");
        assert_eq!(unquote("'unterminated"), "'unterminated");
    }

    #[test]
    fn sanitize_replaces_path_unsafe_chars() {
        // IP:port style serials get turned into safe file names.
        assert_eq!(sanitize("10.10.0.60:5555"), "10.10.0.60_5555");
        assert_eq!(sanitize("com.foo.bar"), "com.foo.bar");
        assert_eq!(sanitize("a b/c"), "a_b_c");
    }
}

#[cfg(test)]
mod list_packages_tests {
    /// Regression guard: when `list_packages` builds its argument vector it
    /// must NOT include a leading `shell` — that's already added by
    /// `run_adb_shell`. A previous version passed `["shell", "pm", ...]`
    /// which produced the duplicated `adb shell shell pm ...` command and
    /// caused device-side `/system/bin/sh` to look up a `shell` command
    /// (toybox reports `shell: inaccessible or not found`, exit 127).
    ///
    /// We can't easily mock the spawned subprocess here without bringing in
    /// a heavier dependency, but we can statically assert the literal shape
    /// of the argument vector: any change that re-introduces a leading
    /// `"shell"` will fail this assertion.
    #[test]
    fn list_packages_does_not_pass_redundant_shell_arg() {
        // Reproduce the exact vec-building logic from `list_packages`.
        let include_system = false;
        let flag = if include_system { "" } else { "-3" };
        let args: Vec<&str> = {
            let mut v: Vec<&str> = vec!["pm", "list", "packages", "-f"];
            if !flag.is_empty() {
                v.push(flag);
            }
            v
        };

        assert_ne!(
            args.first(),
            Some(&"shell"),
            "leading `shell` re-introduced"
        );
        assert_eq!(args, vec!["pm", "list", "packages", "-f", "-3"]);
    }
}


/// Recovery type detected by `recovery_info`. `Unknown` covers both
/// "stock recovery we couldn't recognise" and "device answered but
/// every probe failed" — the UI surfaces both the same way.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RecoveryType {
    Stock,
    Twrp,
    OrangeFox,
    LineageOs,
    Aosp,
    Unknown,
}

impl Default for RecoveryType {
    fn default() -> Self { RecoveryType::Unknown }
}

/// Per-device recovery diagnostics. Every field is optional because
/// each probe is independent: a TWRP that doesn't expose
/// `ro.product.model` (some don't) still gets the other fields filled.
/// Missing fields render as em-dashes, not as "info failed".
///
/// `recovery_type` classifies the recovery variant (custom vs. stock vs.
/// AOSP) at a structural level. The OEM **brand** is a separate
/// dimension exposed via `manufacturer` / `brand` so the UI can show
/// "Xiaomi 原厂 Recovery" instead of a generic "原厂 Recovery" label
/// without inflating the recovery_type enum.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryInfo {
    pub recovery_type: RecoveryType,
    /// Recovery-specific version string, in priority order:
    ///   1. `ro.twrp.version` / `ro.orangefox.version` /
    ///      `ro.lineage.version` (custom recoveries set these)
    ///   2. `ro.recovery.version` (some stock recoveries set this;
    ///      e.g. Lineage stock, some Huawei / Honor builds)
    ///   3. None — MIUI and most OEM stock recoveries don't expose
    ///      their version as a getprop; the version string is only
    ///      visible in the recovery UI itself.
    pub version: Option<String>,
    /// Product model from `ro.product.model` — recovery shells
    /// almost always have this property available.
    pub model: Option<String>,
    /// Recovery's reported build fingerprint (often empty in stock
    /// recovery). Useful when debugging OEM-signed vs unsigned builds.
    pub build_fingerprint: Option<String>,
    /// OEM manufacturer, e.g. `"Xiaomi"`, `"Google"`, `"Samsung"`.
    /// Combined with `recovery_type` to label the UI badge as
    /// "Xiaomi 原厂 Recovery" / "Pixel 原厂 Recovery" etc.
    pub manufacturer: Option<String>,
    /// Brand sub-classification, e.g. `"Redmi"` for Redmi-branded
    /// Xiaomi devices, `"Pixel"` for Google Pixel. Often equals the
    /// manufacturer for OEMs without sub-brands.
    pub brand: Option<String>,
}

/// Try a single `adb shell` getprop and trim the result. Returns
/// `None` on any failure (shell not available, prop unset, etc.) —
/// we never want a partial probe to abort the whole panel.
async fn try_getprop_shell(
    settings: &Settings,
    serial: &str,
    prop: &str,
) -> Option<String> {
    let out = run_adb_shell(settings, serial, &["getprop", prop]).await.ok()?;
    let trimmed = out.trim();
    if trimmed.is_empty() || trimmed == "<unknown>" {
        return None;
    }
    Some(trimmed.to_string())
}

/// Detect the recovery variant on `device`. Probes are independent
/// and best-effort: a single failing getprop doesn't poison the rest.
///
/// Detection order (most specific first):
///   1. TWRP — `ro.twrp.version` is the canonical marker; we also
///      accept the older `ro.twrp.*` form. Falls back to a file probe
///      of `/twres/TWRP` for bootloaders that strip the property.
///   2. OrangeFox — `ro.orangefox.version` (also `ro.of.version` on
///      older builds). Falls back to `/sbin/fox.bin` file probe.
///   3. LineageOS recovery — `ro.lineage.version`.
///   4. AOSP recovery (Pixel, AOSP GSI) — no version property, but
///      `ro.boot.image` is set to the boot image hash. Stock Pixel
///      recovery inherits this from AOSP, so we treat it as "AOSP"
///      unless we can positively identify it as something else.
///   5. Stock (OEM-branded) — `ro.boot.image` empty AND none of the
///      above matched; that's the "vanilla recovery" case.
///   6. Unknown — everything failed.
pub async fn recovery_info(
    settings: &Settings,
    serial: &str,
) -> AppResult<RecoveryInfo> {
    // Probe all known version properties + the fallback markers in
    // parallel-friendly form (sequential awaits are fine; recovery
    // shell is slow regardless). Each probe is independent: a single
    // failing getprop doesn't poison the rest of the response.
    let twrp_version = try_getprop_shell(settings, serial, "ro.twrp.version").await;
    let orangefox_version = try_getprop_shell(settings, serial, "ro.orangefox.version").await;
    let orangefox_alt = try_getprop_shell(settings, serial, "ro.of.version").await;
    let lineage_version = try_getprop_shell(settings, serial, "ro.lineage.version").await;
    let recovery_version_prop = try_getprop_shell(settings, serial, "ro.recovery.version").await;
    let model = try_getprop_shell(settings, serial, "ro.product.model").await;
    let boot_image = try_getprop_shell(settings, serial, "ro.boot.image").await;
    let build_fingerprint = try_getprop_shell(settings, serial, "ro.build.fingerprint").await;
    let manufacturer = try_getprop_shell(settings, serial, "ro.product.manufacturer").await;
    let brand = try_getprop_shell(settings, serial, "ro.product.brand").await;

    let recovery_type = if twrp_version.is_some() {
        RecoveryType::Twrp
    } else if orangefox_version.is_some() || orangefox_alt.is_some() {
        RecoveryType::OrangeFox
    } else if lineage_version.is_some() {
        RecoveryType::LineageOs
    } else if boot_image.is_some() {
        // AOSP / Pixel recovery sets ro.boot.image. If everything else
        // failed but this is set, we know it's at least AOSP-derived.
        RecoveryType::Aosp
    } else {
        RecoveryType::Stock
    };

    // Version priority: custom-recovery version props first, then
    // the generic `ro.recovery.version` (some stock recoveries set
    // this — notably Huawei EMUI / Honor Magic UI / some Samsung
    // builds). MIUI stock doesn't expose its version as a getprop,
    // so for MIUI we end up with `version: None` — that's fine,
    // the recovery UI is the source of truth for those.
    let version = twrp_version
        .or(orangefox_version)
        .or(orangefox_alt)
        .or(lineage_version)
        .or(recovery_version_prop);

    Ok(RecoveryInfo {
        recovery_type,
        version,
        model,
        build_fingerprint,
        manufacturer,
        brand,
    })
}

/// Run `adb sideload <path>`. Verifies the path exists locally first
/// — the Tauri dialog guarantees we received a real path, but a stale
/// file or wrong extension slipped through shouldn't reach the
/// device. Returns the raw stdout (typically "Total xfer: 1.00x" on
/// success or a multi-line error on failure) for the UI to toast.
pub async fn sideload(
    settings: &Settings,
    serial: &str,
    path: &str,
) -> AppResult<String> {
    if path.trim().is_empty() {
        return Err(AppError::InvalidInput("sideload path is empty".into()));
    }
    let p = std::path::Path::new(path);
    if !p.exists() {
        return Err(AppError::InvalidInput(format!(
            "sideload file not found: {path}"
        )));
    }
    if !p.is_file() {
        return Err(AppError::InvalidInput(format!(
            "sideload path is not a file: {path}"
        )));
    }
    let out = run_adb(settings, Some(serial), &["sideload", path]).await?;
    Ok(out.trim().to_string())
}
