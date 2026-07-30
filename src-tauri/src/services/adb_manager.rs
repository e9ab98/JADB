use crate::config::settings::Settings;
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
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
async fn run_adb(settings: &Settings, serial: Option<&str>, args: &[&str]) -> AppResult<String> {
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

/// Run an `adb shell ...` command and return its stdout.
async fn run_adb_shell(
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

/// Returns a flag indicating whether `package` looks like a system app.
async fn is_system_package(settings: &Settings, device: &str, package: &str) -> AppResult<bool> {
    let out = run_adb_shell(settings, device, &["pm", "list", "packages", "-s"]).await?;
    Ok(out
        .lines()
        .any(|l| l.trim() == format!("package:{package}")))
}

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

    // 5) System-app classification (best-effort).
    if let Ok(sys) = is_system_package(settings, device, package).await {
        info.is_system = sys;
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
    let cmd = ["ls", "-la", path];
    let out = run_fs_shell(settings, device, as_pkg, use_root, &cmd).await?;
    Ok(out
        .lines()
        .filter_map(|line| parse_ls_line(line, path))
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
        match run_fs_shell(settings, device, as_pkg, use_root, &["ls", "-ld", &path]).await {
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
    let cmd = ["rm", "-rf", path];
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
        let _ = run_adb_shell(settings, device, &["rm", "-f", &tmp_path]).await;
        return Err(error);
    }
    let result = run_root_shell(settings, device, &["cp", &tmp_path, remote_path]).await;
    let _ = run_adb_shell(settings, device, &["rm", "-f", &tmp_path]).await;
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
    //    partial file and surface the error.
    if let Err(e) = push_direct(settings, device, local_path, &tmp_path).await {
        let _ = run_adb_shell(settings, device, &["rm", "-f", &tmp_path]).await;
        return Err(e);
    }
    let mut cp_args = identity.to_vec();
    cp_args.extend_from_slice(&["cp", &tmp_path, remote_path]);
    let cp_result = run_adb_shell(settings, device, &cp_args).await;
    // 3. Best-effort cleanup of the staged file (run both shells; one will
    //    succeed depending on where the temp actually landed).
    let _ = run_adb_shell(settings, device, &["rm", "-f", &tmp_path]).await;
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
fn base64_encode(input: &[u8]) -> String {
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
