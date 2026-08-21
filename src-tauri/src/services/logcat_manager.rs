//! `adb logcat` capture helpers.
//!
//! Two operations, both one-shot (no live streaming):
//!
//! 1. [`resolve_package_pid`] -- map a package name to its current PID on
//!    the device via `adb shell pidof <pkg>`. Used by the capture command
//!    to feed `--pid=<pid>` into logcat when the user filters by package.
//!
//! 2. [`capture_to_file`] -- run `adb shell logcat -v threadtime [opts] >
//!    <remote_path>` on the device, optionally bounded by a duration, and
//!    return the resulting line count. Two modes:
//!
//!    - `duration_secs == 0`: instant dump (`-d` flag), logcat exits as
//!      soon as the ring buffer has been written. No follow.
//!    - `duration_secs > 0`: streaming mode, no `-d`, killed after the
//!      given window so the file ends at a predictable point.
//!
//! Both modes land the log in `/data/local/tmp/` (writable by the shell
//! uid, no `WRITE_EXTERNAL_STORAGE` needed on Android 11+).

use crate::config::settings::Settings;
use crate::error::{AppError, AppResult};
use std::process::Stdio;
use tokio::process::Command;

/// Look up the running PID of `package` on `device`.
///
/// Returns `None` when the package isn't running (empty stdout from
/// `pidof`). When multiple processes share the package name, returns
/// only the first PID — that's all `--pid=` needs to disambiguate.
pub async fn resolve_package_pid(
    settings: &Settings,
    device: &str,
    package: &str,
) -> AppResult<Option<u32>> {
    let adb = settings
        .adb_path
        .as_deref()
        .ok_or_else(|| AppError::ToolMissing("adb".into()))?
        .to_string();
    let out = Command::new(&adb)
        .arg("-s")
        .arg(device)
        .arg("shell")
        .arg("pidof")
        .arg(package)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| AppError::Config(format!("spawn adb shell pidof: {e}")))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(stdout.split_whitespace().next().and_then(|s| s.parse().ok()))
}

/// Capture logcat output on the device to `remote_path` and return the
/// number of lines written.
///
/// `package`: when `Some`, the stream is filtered with `--pid=<pid>`.
///             Returns `NotFound` if the package isn't currently running,
///             so the UI can surface "launch the app first".
///
/// `duration_secs == 0`: dump current ring buffer and exit (`-d`).
/// `duration_secs >  0`: follow mode, killed after N seconds.
///
/// The capture runs entirely on-device via the shell redirect, so the
/// adb stdout pipe stays empty and we don't pay the cost of streaming
/// every line back to the host.
pub async fn capture_to_file(
    settings: &Settings,
    device: &str,
    package: Option<&str>,
    remote_path: &str,
    duration_secs: u64,
) -> AppResult<u64> {
    // Build the inner shell command. Quote the path so spaces are safe.
    let pid_flag = match package {
        Some(pkg) => match resolve_package_pid(settings, device, pkg).await? {
            Some(pid) => format!(" --pid={pid}"),
            None => {
                return Err(AppError::NotFound(format!(
                    "package {pkg} is not running"
                )));
            }
        },
        None => String::new(),
    };

    let dump_flag = if duration_secs == 0 { " -d" } else { "" };
    let inner = format!(
        "logcat -v threadtime{dump_flag}{pid_flag} > {path}",
        dump_flag = dump_flag,
        pid_flag = pid_flag,
        path = shell_quote(remote_path),
    );

    let adb = settings
        .adb_path
        .as_deref()
        .ok_or_else(|| AppError::ToolMissing("adb".into()))?
        .to_string();
    let mut child = Command::new(&adb)
        .arg("-s")
        .arg(device)
        .arg("shell")
        .arg(&inner)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| AppError::Config(format!("spawn adb logcat: {e}")))?;

    if duration_secs > 0 {
        // Follow mode: kill after the window. `wait` returns when the
        // child exits (kill or natural). We don't care about the exit
        // status — logcat being SIGTERM'd is normal here.
        let timeout = std::time::Duration::from_secs(duration_secs);
        let _ = tokio::time::timeout(timeout, child.wait()).await;
        let _ = child.kill().await;
        let _ = child.wait().await;
    } else {
        // Dump mode: should exit on its own once `-d` drains the buffer.
        // 30s ceiling protects against a wedged logcat that never returns.
        let _ = tokio::time::timeout(std::time::Duration::from_secs(30), child.wait()).await;
    }

    // Count lines that landed on the device. Using `wc -l < <path>` (shell
    // redirect) — the same pattern the apk_sizes_for helper uses.
    let wc = format!("wc -l < {}", shell_quote(remote_path));
    let out = crate::services::adb_manager::run_adb_shell(settings, device, &[&wc]).await?;
    let line_count = out
        .trim()
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    Ok(line_count)
}

/// Minimal POSIX-ish shell quoting so we can interpolate paths into a
/// `adb shell "..."` command without breaking on spaces or quotes.
fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    if value.chars().all(|c| {
        c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '+' | '@')
    }) {
        return value.to_string();
    }
    let escaped = value.replace('\'', "'\\''");
    format!("'{}'", escaped)
}
