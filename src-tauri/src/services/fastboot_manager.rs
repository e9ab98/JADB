//! Fastboot protocol driver.
//!
//! Lives next to `adb_manager` but talks to devices in fastboot /
//! bootloader mode — a separate wire protocol from adb, served by the
//! `fastboot` binary that ships alongside `adb` in Android SDK
//! Platform-Tools. We resolve the fastboot binary from
//! `settings.adb_path`'s parent directory so users don't need a second
//! tool config; if adb is system-installed outside Platform-Tools,
//! `fastboot_binary` returns `AppError::ToolMissing` and the UI shows
//! the "fastboot missing" banner.
//!
//! v1 only exposes the safe subset of fastboot commands — list devices
//! and the three reboot targets (system / recovery / bootloader).
//! Flash / erase / unlock are deliberately NOT wired up here; they
//! require dedicated confirm dialogs and live-log streaming that
//! don't fit the existing single-shot IPC shape.

use crate::config::settings::Settings;
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

/// One row in `fastboot devices -l` output. The wire format is the
/// same shape as `AdbDevice` (serial / state / colon-separated extras)
/// but `state` is always `"fastboot"` — there's no "offline" /
/// "unauthorized" intermediate, the kernel just isn't talking to us
/// until the device shows up here.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FastbootDevice {
    pub serial: String,
    pub state: String,
    pub model: Option<String>,
    pub product: Option<String>,
}

/// Resolve the `fastboot` binary. We assume it lives next to `adb`
/// (the Platform-Tools norm). If `settings.adb_path` is unset or its
/// parent directory does not contain a `fastboot` executable, return
/// `AppError::ToolMissing("fastboot")` so the UI can render the
/// "install Platform-Tools" banner.
fn fastboot_binary(settings: &Settings) -> AppResult<PathBuf> {
    let adb_path = settings
        .adb_path
        .as_deref()
        .ok_or_else(|| AppError::ToolMissing("fastboot".into()))?;
    let dir = std::path::Path::new(adb_path)
        .parent()
        .ok_or_else(|| AppError::Config("adb path has no parent dir".into()))?;
    #[cfg(target_os = "windows")]
    let candidates = ["fastboot.exe"];
    #[cfg(not(target_os = "windows"))]
    let candidates = ["fastboot"];
    for name in candidates {
        let p = dir.join(name);
        if p.exists() {
            return Ok(p);
        }
    }
    Err(AppError::ToolMissing("fastboot".into()))
}

/// Run a fastboot subcommand. `serial` is an optional `-s <device>`
/// selector; pass `None` for queries like `devices` that span every
/// attached device. Mirrors the structure of
/// `adb_manager::run_adb` so error handling stays uniform.
pub(crate) async fn run_fastboot(
    settings: &Settings,
    serial: Option<&str>,
    args: &[&str],
) -> AppResult<String> {
    let fb = fastboot_binary(settings)?;
    let mut cmd = Command::new(&fb);
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
        .map_err(|e| AppError::Config(format!("spawn fastboot: {e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        return Err(AppError::ToolFailed {
            tool: "fastboot".into(),
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

/// List fastboot devices. Mirrors `adb_manager::list_devices` parsing
/// — `<serial>\t<state>\t[product:... model:...]` rows after the
/// `List of devices` header.
pub async fn list_devices(settings: &Settings) -> AppResult<Vec<FastbootDevice>> {
    let out = run_fastboot(settings, None, &["devices", "-l"]).await?;
    let mut devices = Vec::new();
    for line in out.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("List of devices") {
            continue;
        }
        let mut parts = line.split_whitespace();
        let serial = match parts.next() {
            Some(s) => s.to_string(),
            None => continue,
        };
        let state = parts.next().unwrap_or("fastboot").to_string();
        let mut model = None;
        let mut product = None;
        for tok in parts {
            if let Some(v) = tok.strip_prefix("model:") {
                model = Some(v.replace('_', " "));
            } else if let Some(v) = tok.strip_prefix("product:") {
                product = Some(v.to_string());
            }
        }
        devices.push(FastbootDevice {
            serial,
            state,
            model,
            product,
        });
    }
    Ok(devices)
}

/// Reboot `device`. `mode` is `None` for a normal reboot (back to
/// system), `Some("recovery")` for recovery, `Some("bootloader")` to
/// stay in fastboot. Any unknown mode returns `AppError::InvalidInput`
/// — same shape as `adb_manager::power_reboot`.
pub async fn reboot(
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
                    "unknown fastboot reboot mode: {other}"
                )));
            }
        }
    }
    let out = run_fastboot(settings, Some(device), &args).await?;
    Ok(out.trim().to_string())
}


/// Subset of bootloader variables surfaced in the UI's "Get info"
/// panel. Every field is `Option<String>` because individual `getvar`
/// calls can fail (variable not supported on this device / bootloader
/// build, or the bootloader returned non-zero for that var). Missing
/// fields should render as em-dashes in the UI rather than breaking
/// the whole panel.
///
/// Field ordering here drives the order the variables appear in the
/// UI's 2-column grid — keep this grouped: identity → versions →
/// flashing pre-flight → slot info.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct FastbootVarInfo {
    // --- Identity & security ----------------------------------------
    /// `getvar unlocked` → `"yes"` / `"no"`. Most-asked variable; the
    /// UI highlights this with a green/red badge.
    pub unlocked: Option<String>,
    /// `getvar verified-boot-state` → `"green"` / `"yellow"` /
    /// `"orange"` / `"red"`. Finer-grained than the deprecated
    /// `secureboot` yes/no — `yellow` means a user-built OS booted
    /// but verified, `red` means verification failed.
    pub verified_boot_state: Option<String>,
    /// `getvar hardware` — chip family / SoC identifier
    /// (`"qcom"`, `"exynos"`, `"mt6895"`, `"gs201"`, ...).
    pub hardware: Option<String>,
    /// `getvar variant` — product variant within a codename
    /// (`"pro"`, `"max"`, `"ultra"`). Often `None` on devices that
    /// only ship one SKU.
    pub variant: Option<String>,
    // --- Versions ---------------------------------------------------
    /// `getvar version-bootloader` — human-readable bootloader
    /// version string.
    pub version_bootloader: Option<String>,
    /// `getvar version-hardware` — hardware revision (`"rev_03"`,
    /// `"EVT1.5"`, ...).
    pub version_hardware: Option<String>,
    /// `getvar version-baseband` — baseband/modem version.
    pub version_baseband: Option<String>,
    /// `getvar product` — product codename (e.g. `"bramble"`,
    /// `"panther"`, `"raphael"`).
    pub product: Option<String>,
    // --- Flashing pre-flight ----------------------------------------
    /// `getvar max-download-size` — max image size the bootloader
    /// will accept over USB, as a hex string (e.g. `"0x80000000"` =
    /// 2 GB). Critical for users about to flash large AOSP bundles.
    pub max_download_size: Option<String>,
    /// `getvar off-mode-charge` → `"yes"` / `"no"`. Whether USB
    /// charging works when the device is powered off. A `no` here
    /// means a failed flash can drain the battery before you can
    /// retry.
    pub off_mode_charge: Option<String>,
    /// `getvar battery-soc-ok` → `"yes"` / `"no"`. Some bootloaders
    /// (Pixel, OnePlus) gate flashing on battery SoC and report this
    /// so the host can warn the user.
    pub battery_soc_ok: Option<String>,
    /// `getvar anti-rollback` (with `getvar anti` as a fallback for
    /// bootloaders that use the short name) — anti-rollback index.
    /// A higher-than-installed index blocks downgrades.
    pub anti_rollback: Option<String>,
    // --- Slot info --------------------------------------------------
    /// `getvar current-slot` — `_a` / `_b` for A/B devices.
    pub current_slot: Option<String>,
    /// `getvar slot-count` — `1` (non-AB) / `2` (AB).
    pub slot_count: Option<String>,
    /// `getvar serialno` — device serial as reported by the
    /// bootloader (sometimes differs from the adb-mode serial).
    pub serialno: Option<String>,
}

/// Try `fastboot getvar <name>`. Returns `Some(value)` if the bootloader
/// printed `<name>: <value>`; `None` on any failure (binary missing,
/// exit non-zero, output unparseable). Fastboot formats output as
/// `"<name>: <value>"` on success and `"getvar:Variable not
/// supported"` (or similar) on failure — we look for a `<name>:` prefix
/// line so spurious stderr noise (some bootloaders print warnings) is
/// ignored.
async fn try_getvar(
    settings: &Settings,
    device: &str,
    name: &str,
) -> Option<String> {
    let out = run_fastboot(settings, Some(device), &["getvar", name])
        .await
        .ok()?;
    for line in out.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(name) {
            if let Some(value) = rest.trim_start().strip_prefix(':') {
                return Some(value.trim().to_string());
            }
        }
    }
    None
}

/// Fetch the full `FastbootVarInfo` for `device`. Each var is queried
/// independently — one missing/unavailable variable doesn't poison the
/// rest of the panel. Sequential awaits are fine here: 8 vars × ~50 ms
/// each = ~400 ms worst case on a healthy device, and the UI shows a
/// spinner while the call is in flight.
pub async fn get_info(
    settings: &Settings,
    device: &str,
) -> AppResult<FastbootVarInfo> {
    // `anti-rollback` is reported as `anti` on a handful of older
    // bootloaders (e.g. early MSM kernels). Try the canonical name
    // first, then fall back to the short alias so we get a value on
    // either convention.
    let anti_rollback = match try_getvar(settings, device, "anti-rollback").await {
        Some(v) => Some(v),
        None => try_getvar(settings, device, "anti").await,
    };
    Ok(FastbootVarInfo {
        // identity & security
        unlocked: try_getvar(settings, device, "unlocked").await,
        verified_boot_state: try_getvar(settings, device, "verified-boot-state")
            .await,
        hardware: try_getvar(settings, device, "hardware").await,
        variant: try_getvar(settings, device, "variant").await,
        // versions
        version_bootloader: try_getvar(settings, device, "version-bootloader")
            .await,
        version_hardware: try_getvar(settings, device, "version-hardware")
            .await,
        version_baseband: try_getvar(settings, device, "version-baseband").await,
        product: try_getvar(settings, device, "product").await,
        // flashing pre-flight
        max_download_size: try_getvar(settings, device, "max-download-size")
            .await,
        off_mode_charge: try_getvar(settings, device, "off-mode-charge").await,
        battery_soc_ok: try_getvar(settings, device, "battery-soc-ok").await,
        anti_rollback,
        // slot info
        current_slot: try_getvar(settings, device, "current-slot").await,
        slot_count: try_getvar(settings, device, "slot-count").await,
        serialno: try_getvar(settings, device, "serialno").await,
    })
}


/// Output of `fastboot oem device-info`. **Raw, unparsed** — different
/// vendors emit wildly different keys (Pixel uses
/// `(bootloader) Device tampered: false`, Xiaomi uses a different
/// subset, Huawei doesn't support the command at all). We deliberately
/// don't try to normalise here; the frontend picks the OEM-specific
/// parser based on `FastbootDevice.product`. This makes adding a new
/// OEM a frontend-only change — no Rust retouch needed.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct OemDeviceInfo {
    /// `fastboot oem device-info` stdout split into lines. The
    /// optional `(bootloader) ` prefix is preserved so the frontend
    /// parser can do its own line classification.
    pub raw_lines: Vec<String>,
}

/// Run `fastboot oem device-info` on `device` and return the raw
/// stdout. We let `run_fastboot`'s normal error handling bubble up:
/// bootloaders that don't support the command (Huawei, MTK, some
/// older MediaTek chipsets) exit non-zero with "unknown command" in
/// stderr, which the frontend catches and renders as "not supported".
pub async fn oem_device_info(
    settings: &Settings,
    device: &str,
) -> AppResult<OemDeviceInfo> {
    let out = run_fastboot(settings, Some(device), &["oem", "device-info"]).await?;
    Ok(OemDeviceInfo {
        raw_lines: out.lines().map(|s| s.to_string()).collect(),
    })
}
