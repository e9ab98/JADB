use crate::config::settings;
use crate::config::settings::Settings;use crate::error::{AppError, AppResult};
use crate::services::adb_agent;
use crate::services::adb_manager::{self, AdbDevice, AppInfo, DeviceSystemInfo, DirEntry, ExportApksResult, InstallApksResult, RecoveryInfo};
use tauri::{AppHandle, Manager, State};
use crate::services::license::{LicenseService, FEATURE_ADB_BATCH_INSTALL};

#[tauri::command]
pub async fn adb_devices(app: AppHandle) -> AppResult<Vec<AdbDevice>> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::list_devices(&s).await
}

#[tauri::command]
pub async fn adb_connect(
    app: AppHandle,
    host: String,
    port: u16,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::connect_wifi(&s, &host, port).await
}

#[tauri::command]
pub async fn adb_disconnect(
    app: AppHandle,
    target: Option<String>,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::disconnect(&s, target.as_deref()).await
}

#[tauri::command]
pub async fn adb_list_packages(
    app: AppHandle,
    device: String,
    include_system: bool,
) -> AppResult<Vec<String>> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::list_packages(&s, &device, include_system).await
}

#[tauri::command]
pub async fn adb_app_info(
    app: AppHandle,
    device: String,
    package: String,
) -> AppResult<AppInfo> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::package_info(&app, &s, &device, &package).await
}

/// Lightweight per-package info: `dumpsys package` + `pm path` + `wc`
/// only. **No APK pull, no aapt2.** Same AppInfo shape as the heavy
/// `adb_app_info`, but ~5 IPCs total instead of ~5 IPCs + ~MB-scale pull
/// + aapt2 spawn. Use this for the per-card enrichment waterfall in
/// AdbAppsTab; the heavy path is only needed when the caller actually
/// wants the APK file on disk (e.g. for jadx / analyze / decompile).
#[tauri::command]
pub async fn adb_app_info_lite(
    app: AppHandle,
    device: String,
    package: String,
) -> AppResult<AppInfo> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::package_info_lite(&app, &s, &device, &package).await
}

#[tauri::command]
pub async fn adb_app_icon(
    app: AppHandle,
    device: String,
    package: String,
) -> AppResult<Option<String>> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::pull_app_icon(&app, &s, &device, &package).await
}

#[tauri::command]
pub async fn adb_install_apks(
    app: AppHandle,
    license: State<'_, LicenseService>,
    device: String,
    paths: Vec<String>,
) -> AppResult<InstallApksResult> {
    if paths.len() > 1 {
        license.require_feature(&app, FEATURE_ADB_BATCH_INSTALL).await?;
    }
    let dir = app.path().app_data_dir().map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::install_apks(&s, &device, &paths).await
}

#[tauri::command]
pub async fn adb_uninstall(
    app: AppHandle,
    device: String,
    package: String,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::uninstall(&s, &device, &package).await
}


#[tauri::command]
pub async fn adb_apk_paths(app: AppHandle, device: String, package: String) -> AppResult<Vec<String>> {
    let dir = app.path().app_data_dir().map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::apk_paths_for(&s, &device, &package).await
}

#[tauri::command]
pub async fn adb_pull_apk_for_tool(app: AppHandle, device: String, package: String, remote_path: String) -> AppResult<String> {
    let dir = app.path().app_data_dir().map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::pull_apk_to_cache(&app, &s, &device, &package, &remote_path).await
}

#[tauri::command]
pub async fn adb_export_apks(
    app: AppHandle,
    device: String,
    package: String,
    version_name: Option<String>,
    target_dir: String,
) -> AppResult<ExportApksResult> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| AppError::Config(error.to_string()))?;
    let settings = settings::read(&dir).await?;
    adb_manager::export_apks(
        &settings,
        &device,
        &package,
        version_name.as_deref(),
        &target_dir,
    )
    .await
}

#[tauri::command]
pub async fn adb_force_stop(
    app: AppHandle,
    device: String,
    package: String,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::force_stop(&s, &device, &package).await
}

/// Reboot `device`. `mode` is `None` for a normal reboot, `Some("recovery")`
/// to drop into recovery, `Some("bootloader")` to drop into fastboot.
#[tauri::command]
pub async fn adb_reboot(
    app: AppHandle,
    device: String,
    mode: Option<String>,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::power_reboot(&s, &device, mode.as_deref()).await
}

/// Power off `device`. Tries `adb reboot -p` first; falls back to a
/// power-button keyevent if the device rejects the syscall (so the
/// screen goes off even on builds that lock down the reboot call).
#[tauri::command]
pub async fn adb_shutdown(
    app: AppHandle,
    device: String,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::power_shutdown(&s, &device).await
}

#[tauri::command]
pub async fn adb_launch_app(
    app: AppHandle,
    device: String,
    package: String,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::launch_app(&s, &device, &package).await
}

#[tauri::command]
pub async fn adb_clear_cache(
    app: AppHandle,
    device: String,
    package: String,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::clear_cache(&s, &device, &package).await
}

#[tauri::command]
pub async fn is_device_rooted(
    app: AppHandle,
    device: String,
) -> AppResult<bool> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::is_device_rooted(&s, &device).await
}

#[tauri::command]
pub async fn pull_file(
    app: AppHandle,
    device: String,
    remote_path: String,
    local_path: String,
    as_pkg: Option<String>,
    use_root: bool,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::download_file(
        &s,
        &device,
        &remote_path,
        &local_path,
        as_pkg.as_deref(),
        use_root,
    )
    .await
}

#[tauri::command]
pub async fn list_remote_dir(
    app: AppHandle,
    device: String,
    path: String,
    as_pkg: Option<String>,
    use_root: bool,
) -> AppResult<Vec<DirEntry>> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::list_remote_dir(&s, &device, &path, as_pkg.as_deref(), use_root).await
}

#[tauri::command]
pub async fn resolve_app_data_dir(
    app: AppHandle,
    device: String,
    package: String,
    as_pkg: Option<String>,
    use_root: bool,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::resolve_app_data_dir(
        &s,
        &device,
        &package,
        as_pkg.as_deref(),
        use_root,
    )
    .await
}

#[tauri::command]
pub async fn delete_remote_file(
    app: AppHandle,
    device: String,
    path: String,
    as_pkg: Option<String>,
    use_root: bool,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::delete_remote_file(&s, &device, &path, as_pkg.as_deref(), use_root).await
}

#[tauri::command]
pub async fn push_file(
    app: AppHandle,
    device: String,
    local_path: String,
    remote_path: String,
    as_pkg: Option<String>,
    use_root: bool,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::push_file(
        &s,
        &device,
        &local_path,
        &remote_path,
        as_pkg.as_deref(),
        use_root,
    )
    .await
}

use crate::services::adb_manager::ShellOutput;

#[tauri::command]
pub async fn adb_shell(
    app: AppHandle,
    device: String,
    command: String,
) -> AppResult<ShellOutput> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::shell_exec(&s, &device, &command).await
}


#[tauri::command]
pub async fn adb_system_info(
    app: AppHandle,
    device: String,
) -> AppResult<DeviceSystemInfo> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::system_info(&s, &device).await
}

/// Same as [`adb_system_info`] but runs the on-device agent first to fill
/// the bulk of the fields via Binder (Build.* + SystemProperties ro.* +
/// ActivityManager memory + WifiManager + BatteryManager + Configuration),
/// then runs the shell commands only for fields the agent couldn't fill
/// (CPU details, kernel version, dumpsys SurfaceFlinger for GPU, etc).
///
/// Falls back to the slow all-shell path if the agent fails for any
/// reason (push error, dex crash, OEM blocking hidden APIs).
#[tauri::command]
pub async fn adb_system_info_via_agent(
    app: AppHandle,
    device: String,
) -> AppResult<DeviceSystemInfo> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;

    // Fast path: ask the agent. It returns a JSON object with everything
    // it could gather. Anything missing falls back to shell.
    let agent_json = match adb_agent::run_agent_sysinfo(&s, &device).await {
        Ok(v) => v,
        Err(e) => {
            log::warn!(
                "adb_system_info_via_agent: agent failed ({e}); falling back to shell-only path"
            );
            return adb_manager::system_info(&s, &device).await;
        }
    };

    // Always do the shell pass too, but only set fields the agent
    // didn't provide. The shell pass is ~17 IPCs (down from 46); agent
    // typically fills 30+ fields so we mostly just hit missing ones.
    let mut info = DeviceSystemInfo::default();
    let agent_obj = agent_json.as_object();
    let get = |k: &str| -> Option<String> {
        agent_obj
            .and_then(|o| o.get(k))
            .and_then(|v| v.as_str().map(String::from))
    };

    // --- Hardware: agent fills all ---
    info.manufacturer = get("Build.MANUFACTURER");
    info.brand = get("Build.BRAND");
    info.model = get("Build.MODEL");
    info.device = get("Build.DEVICE");
    info.hardware = get("Build.HARDWARE");
    info.platform = get("ro.board.platform").or_else(|| get("ro.product.board"));
    info.serial = get("Build.SERIAL").or_else(|| get("ro.serialno"));
    info.bootloader = get("Build.BOOTLOADER").or_else(|| get("ro.bootloader"));
    info.fingerprint = get("Build.FINGERPRINT");

    // --- Display: agent has Configuration/DisplayMetrics/WindowManager. ---
    // The Rust struct still has free-form strings here (parsed downstream),
    // so we just hand the agent's int values back as strings.
    info.screen_size = {
        let w = agent_obj
            .and_then(|o| o.get("DisplayMetrics.widthPixels"))
            .and_then(|v| v.as_i64());
        let h = agent_obj
            .and_then(|o| o.get("DisplayMetrics.heightPixels"))
            .and_then(|v| v.as_i64());
        match (w, h) {
            (Some(w), Some(h)) => Some(format!("{w}x{h}")),
            _ => None,
        }
    };
    info.screen_density = agent_obj
        .and_then(|o| o.get("DisplayMetrics.density"))
        .and_then(|v| v.as_f64())
        .map(|d| format!("{}", d));
    info.screen_refresh_rate = agent_obj
        .and_then(|o| o.get("Display.refreshRate"))
        .and_then(|v| v.as_f64())
        .map(|r| format!("{}", r));
    info.physical_size = {
        let w = agent_obj.and_then(|o| o.get("Display.realWidth")).and_then(|v| v.as_i64());
        let h = agent_obj.and_then(|o| o.get("Display.realHeight")).and_then(|v| v.as_i64());
        match (w, h) {
            (Some(w), Some(h)) => Some(format!("{w}x{h}")),
            _ => None,
        }
    };
    info.rotation = agent_obj
        .and_then(|o| o.get("Display.rotation"))
        .and_then(|v| v.as_i64())
        .map(|r| r.to_string());

    // --- System: agent has everything we need ---
    info.android_release = get("Build.VERSION.RELEASE");
    info.android_sdk = agent_obj
        .and_then(|o| o.get("Build.VERSION.SDK_INT"))
        .and_then(|v| v.as_i64())
        .map(|n| n.to_string());
    info.security_patch = get("Build.VERSION.SECURITY_PATCH");
    info.build_id = get("Build.ID");
    info.build_type = get("Build.TYPE");
    info.kernel_version = None; // filled by shell pass below
    info.java_vm = get("ro.java.vm.version")
        .or_else(|| get("ro.dalvik.vm.version"));
    info.abi = get("ro.product.cpu.abi");
    info.abi_list = get("ro.product.cpu.abilist")
        .or_else(|| get("ro.product.cpu.abilist64"));

    // --- CPU: agent has cores; rest via shell ---
    info.cpu_cores = None; // filled by shell pass below

    // --- GPU: agent has vulkan + opengles + gralloc; full renderer string via shell ---
    info.gpu_opengles_version = get("ro.opengles.version");
    // gpu_vendor / gpu_renderer / gpu_vulkan_version / gpu_driver filled by shell

    // --- Memory: agent has totalMem/availMem; rest via shell ---
    info.ram_total = agent_obj
        .and_then(|o| o.get("Memory.totalMem"))
        .and_then(|v| v.as_i64())
        .map(|n| n.to_string());
    info.ram_available = agent_obj
        .and_then(|o| o.get("Memory.availMem"))
        .and_then(|v| v.as_i64())
        .map(|n| n.to_string());

    // --- Storage: shell only ---
    info.storage_total = None;
    info.storage_available = None;

    // --- Network: agent has SSID/IP/signal/link speed/frequency/type ---
    info.wifi_ssid = get("Wifi.ssid");
    info.wifi_ip = get("Wifi.ipAddress");
    info.wifi_signal = agent_obj
        .and_then(|o| o.get("Wifi.rssi"))
        .and_then(|v| v.as_i64())
        .map(|n| n.to_string());
    info.wifi_link_speed = agent_obj
        .and_then(|o| o.get("Wifi.linkSpeed"))
        .and_then(|v| v.as_i64())
        .map(|n| n.to_string());
    info.wifi_frequency = agent_obj
        .and_then(|o| o.get("Wifi.frequency"))
        .and_then(|v| v.as_i64())
        .map(|n| n.to_string());
    info.network_type = get("Network.type").or_else(|| get("Network.subtype"));
    info.operator = None; // shell
    info.airplane_mode = None; // shell
    info.ipv4 = None; // shell

    // --- Battery: agent has level/status/health/plugged/temp/voltage/tech ---
    info.battery_level = agent_obj
        .and_then(|o| o.get("Battery.level"))
        .and_then(|v| v.as_i64())
        .map(|n| n.to_string());
    info.battery_status = get("Battery.status");
    info.battery_health = get("Battery.health");
    info.battery_temp = agent_obj
        .and_then(|o| o.get("Battery.temperature"))
        .and_then(|v| v.as_i64())
        .map(|n| n.to_string());
    info.battery_voltage = agent_obj
        .and_then(|o| o.get("Battery.voltage"))
        .and_then(|v| v.as_i64())
        .map(|n| n.to_string());
    info.battery_technology = get("Battery.technology");
    info.battery_plugged = get("Battery.plugged");

    // --- Runtime: agent has TimeZone/Locale, rest via shell ---
    info.uptime = None; // shell
    info.boot_time = None; // shell
    info.selinux = get("SELinux.enforce").or_else(|| get("ro.boot.selinux"));
    info.timezone = get("TimeZone.id");
    info.locale = get("Locale.toString").or_else(|| get("Configuration.locale"));
    info.foreground_app = None; // shell
    info.screen_state = None; // shell

    // Shell pass for fields the agent couldn't fill. We only run targeted
    // shell commands for the missing fields, not the full 46-IPC
    // `system_info` sweep, so we keep the agent's speedup.
    fill_shell_if_missing(&s, &device, &mut info).await?;
    Ok(info)
}

/// Run individual `adb shell` commands only for fields the agent JSON
/// couldn't fill. Avoids the full 46-IPC `system_info` sweep.
async fn fill_shell_if_missing(
    s: &Settings,
    device: &str,
    info: &mut DeviceSystemInfo,
) -> AppResult<()> {
    // (label on Rust side, shell command, target field)
    async fn fetch(s: &Settings, device: &str, args: &[&str]) -> AppResult<String> {
        crate::services::adb_manager::run_adb_shell(s, device, args).await
    }

    // CPU
    if info.cpu_cores.is_none() {
        if let Ok(o) = fetch(s, device, &["nproc"]).await {
            info.cpu_cores = Some(o.trim().to_string());
        }
    }
    if info.cpu_hardware.is_none() || info.cpu_features.is_none() {
        if let Ok(o) = fetch(s, device, &["cat", "/proc/cpuinfo"]).await {
            for line in o.lines() {
                if let Some(v) = line.strip_prefix("Hardware	: ") {
                    if info.cpu_hardware.is_none() { info.cpu_hardware = Some(v.trim().to_string()); }
                } else if let Some(v) = line.strip_prefix("Features	: ") {
                    if info.cpu_features.is_none() { info.cpu_features = Some(v.trim().to_string()); }
                }
            }
        }
    }
    if info.cpu_max_freq.is_none() {
        if let Ok(o) = fetch(s, device,
            &["cat", "/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq"]).await
        {
            info.cpu_max_freq = Some(o.trim().to_string());
        }
    }

    // Kernel
    if info.kernel_version.is_none() {
        if let Ok(o) = fetch(s, device, &["uname", "-r"]).await {
            info.kernel_version = Some(o.trim().to_string());
        }
    }

    // GPU (renderer / vendor / vulkan) -- dumpsys SurfaceFlinger has it
    if info.gpu_vendor.is_none() || info.gpu_renderer.is_none()
        || info.gpu_vulkan_version.is_none() || info.gpu_driver.is_none()
    {
        if let Ok(o) = fetch(s, device,
            &["dumpsys", "SurfaceFlinger"]).await
        {
            for line in o.lines() {
                let t = line.trim();
                if let Some(v) = t.strip_prefix("GLES: ") {
                    if info.gpu_vendor.is_none() { info.gpu_vendor = Some(v.trim().to_string()); }
                } else if let Some(v) = t.strip_prefix("GLES Vendor: ") {
                    if info.gpu_vendor.is_none() { info.gpu_vendor = Some(v.trim().to_string()); }
                } else if let Some(v) = t.strip_prefix("Renderer: ") {
                    if info.gpu_renderer.is_none() { info.gpu_renderer = Some(v.trim().to_string()); }
                } else if let Some(v) = t.strip_prefix("Vulkan: ") {
                    if info.gpu_vulkan_version.is_none() { info.gpu_vulkan_version = Some(v.trim().to_string()); }
                } else if let Some(v) = t.strip_prefix("Vulkan API: ") {
                    if info.gpu_vulkan_version.is_none() { info.gpu_vulkan_version = Some(v.trim().to_string()); }
                } else if let Some(v) = t.strip_prefix("Driver: ") {
                    if info.gpu_driver.is_none() { info.gpu_driver = Some(v.trim().to_string()); }
                }
            }
        }
    }

    // Storage
    if info.storage_total.is_none() || info.storage_available.is_none() {
        if let Ok(o) = fetch(s, device,
            &["sh", "-c",
             "df /data 2>/dev/null | tail -1 | awk '{print $2, $4}'"]).await
        {
            let parts: Vec<&str> = o.split_whitespace().collect();
            if let Some(t) = parts.first() {
                if info.storage_total.is_none() { info.storage_total = Some(format!("{} KB", t)); }
            }
            if let Some(a) = parts.get(1) {
                if info.storage_available.is_none() { info.storage_available = Some(format!("{} KB", a)); }
            }
        }
    }

    // Network (operator / airplane / ipv4)
    if info.operator.is_none() {
        if let Ok(o) = fetch(s, device,
            &["dumpsys", "telephony.registry"]).await
        {
            for line in o.lines() {
                if let Some(v) = line.trim().strip_prefix("mOperatorAlpha= ") {
                    if info.operator.is_none() { info.operator = Some(v.to_string()); break; }
                }
            }
        }
    }
    if info.airplane_mode.is_none() {
        if let Ok(o) = fetch(s, device,
            &["settings", "get", "global", "airplane_mode_on"]).await
        {
            let v = o.trim();
            info.airplane_mode = Some(if v == "1" { "On".into() } else { "Off".into() });
        }
    }
    if info.ipv4.is_none() {
        // Try common interfaces
        for iface in ["wlan0", "eth0", "rmnet0"] {
            if let Ok(o) = fetch(s, device,
                &["ip", "-4", "addr", "show", iface]).await
            {
                for line in o.lines() {
                    if let Some(rest) = line.trim().strip_prefix("inet ") {
                        if let Some(addr) = rest.split_whitespace().next() {
                            if addr != "127.0.0.1" {
                                info.ipv4 = Some(addr.to_string());
                                break;
                            }
                        }
                    }
                }
                if info.ipv4.is_some() { break; }
            }
        }
    }

    // Runtime
    if info.uptime.is_none() || info.boot_time.is_none() {
        if let Ok(o) = fetch(s, device, &["cat", "/proc/uptime"]).await {
            let parts: Vec<&str> = o.split_whitespace().collect();
            if let Some(secs_str) = parts.first() {
                if let Ok(secs) = secs_str.parse::<f64>() {
                    if info.uptime.is_none() {
                        info.uptime = Some(format_uptime_secs(secs as u64));
                    }
                    if info.boot_time.is_none() {
                        info.boot_time = Some(boot_time_from_uptime(secs as u64));
                    }
                }
            }
        }
    }
    if info.screen_state.is_none() {
        if let Ok(o) = fetch(s, device, &["dumpsys", "power"]).await {
            for line in o.lines() {
                if let Some(v) = line.trim().strip_prefix("mWakefulness=") {
                    info.screen_state = Some(v.trim().to_string());
                    break;
                }
            }
        }
    }
    if info.foreground_app.is_none() {
        if let Ok(o) = fetch(s, device, &["dumpsys", "activity", "activities"]).await {
            for line in o.lines() {
                if let Some(v) = line.trim().strip_prefix("ResumedActivity:") {
                    if let Some(rest) = v.split_whitespace().next() {
                        if rest.contains('/') {
                            info.foreground_app = Some(rest.to_string());
                            break;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn format_uptime_secs(secs: u64) -> String {
    // Same formatter as the one in adb_manager::system_info. Inlined
    // here so we don't have to expose the private helper.
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3_600;
    let mins = (secs % 3_600) / 60;
    let s = secs % 60;
    if days > 0 {
        format!("{}d {}h {}m {}s", days, hours, mins, s)
    } else if hours > 0 {
        format!("{}h {}m {}s", hours, mins, s)
    } else {
        format!("{}m {}s", mins, s)
    }
}

fn boot_time_from_uptime(uptime_secs: u64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let boot = now.saturating_sub(uptime_secs);
    // Format as YYYY-MM-DD HH:MM:SS UTC (we don't know the device's
    // timezone from uptime alone, but the SystemInfoDumper does report
    // TimeZone.id separately so we could use that in the future).
    format_utc_datetime(boot)
}

/// Format a unix epoch (seconds) as `YYYY-MM-DD HH:MM:SS` UTC.
fn format_utc_datetime(epoch_secs: u64) -> String {
    let secs_per_day = 86_400;
    let days = epoch_secs / secs_per_day;
    let rem = epoch_secs % secs_per_day;
    let h = rem / 3_600;
    let m = (rem % 3_600) / 60;
    let s = rem % 60;
    // 1970-01-01 is the epoch. Days since then. Compute year/month/day
    // by walking forward -- this avoids dragging chrono in for one call.
    let mut year: i64 = 1970;
    let mut days_left = days as i64;
    loop {
        let leap = is_leap(year);
        let year_days = if leap { 366 } else { 365 };
        if days_left < year_days { break; }
        days_left -= year_days;
        year += 1;
    }
    let leap = is_leap(year);
    let months = [31, if leap {29} else {28}, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1;
    for (i, &md) in months.iter().enumerate() {
        if days_left < md { month = i + 1; break; }
        days_left -= md;
    }
    let day = days_left + 1;
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", year, month, day, h, m, s)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

use crate::services::logcat_manager;

/// Capture logcat output on the device to a file and return how many
/// lines were written.
///
/// `package` is optional — when provided, the stream is filtered to that
/// PID (resolved on the device via `pidof`). The capture uses a shell
/// redirect so the host never sees individual lines; only the final
/// line count is returned.
///
/// `duration_secs == 0` dumps the current ring buffer (`-d`, instant).
/// `duration_secs >  0` follows for N seconds and is killed at the end.
/// `remote_path` should live under `/data/local/tmp/` — that directory
/// is writable by the shell uid on every Android version, so the
/// capture doesn't need any storage permission.
#[tauri::command]
pub async fn adb_logcat_capture(
    app: AppHandle,
    device: String,
    package: Option<String>,
    remote_path: String,
    duration_secs: u64,
) -> AppResult<u64> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    logcat_manager::capture_to_file(
        &s,
        &device,
        package.as_deref(),
        &remote_path,
        duration_secs,
    )
    .await
}

/// Pull a previously-captured log file from the device to a local path
/// chosen by the user (typically via the system save dialog). Returns
/// the local path so the caller can show "Saved to ...".
#[tauri::command]
pub async fn adb_logcat_pull(
    app: AppHandle,
    device: String,
    remote_path: String,
    local_path: String,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::pull_file(
        &s,
        &device,
        &remote_path,
        std::path::Path::new(&local_path),
    )
    .await?;
    Ok(local_path)
}

/// Fast path: dump every installed package in one shot via the on-device
/// agent. Returns the same `AppInfo` shape as `adb_app_info` so the
/// frontend can render the app list immediately while icons are still
/// being pulled in the background.
/// Best-effort package list. Tries the on-device agent (fast path)
/// and falls back to `pm list packages -f` via shell (slow path, but
/// works everywhere). The agent can fail for many reasons: framework
/// JNI signature mismatch on the specific Android build, OEM-modified
/// `app_process`, SELinux blocking, etc. The slow path is `adb shell pm
/// list packages -f` which only needs shell uid.
#[tauri::command]
pub async fn adb_list_packages_via_agent(
    app: AppHandle,
    device: String,
) -> AppResult<Vec<AppInfo>> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;

    // Fast path: agent dex via app_process. Best-effort.
    if let Ok(()) = adb_agent::ensure_agent_pushed(&app, &s, &device).await {
        match adb_agent::run_agent(&s, &device, &[]).await {
            Ok(stdout) => {
                let parsed = adb_agent::parse_agent_output(&stdout);
                if !parsed.is_empty() {
                    return Ok(parsed);
                }
                log::warn!(
                    "adb_list_packages_via_agent: agent returned 0 packages, falling back to shell"
                );
            }
            Err(e) => {
                log::warn!(
                    "adb_list_packages_via_agent: agent failed ({e}), falling back to shell"
                );
            }
        }
    } else {
        log::warn!("adb_list_packages_via_agent: agent push failed, falling back to shell");
    }

    // Slow path: `pm list packages -f` via shell. We map each line to a
    // minimal `AppInfo`; metadata fields (version, icon, apk size) stay
    // `None` and get filled later by per-package enrichment.
    adb_manager::list_packages_via_shell(&s, &device, false).await
}

/// Pull a single icon PNG that the agent previously wrote to
/// `/data/local/tmp/jadb-icons/icons/<pkg>.png` and return it as a
/// `data:image/png;base64,...` URL. Returns `None` when the agent didn't
/// render an icon for the package (e.g. system apps with no icon res).
#[tauri::command]
pub async fn adb_app_icon_via_agent(
    app: AppHandle,
    device: String,
    package: String,
) -> AppResult<Option<String>> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_agent::ensure_agent_pushed(&app, &s, &device).await?;
    adb_agent::pull_icon(&s, &device, &package).await
}

#[tauri::command]
pub async fn adb_recovery_info(
    app: AppHandle,
    device: String,
) -> AppResult<RecoveryInfo> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::recovery_info(&s, &device).await
}

#[tauri::command]
pub async fn adb_sideload(
    app: AppHandle,
    device: String,
    path: String,
) -> AppResult<String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    adb_manager::sideload(&s, &device, &path).await
}
