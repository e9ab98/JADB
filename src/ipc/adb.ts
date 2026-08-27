import { invoke } from '@tauri-apps/api/core';

export type AdbDevice = {
  serial: string;
  state: string;
  model: string | null;
  product: string | null;
  transport: string | null;
};

export type AppInfo = {
  packageName: string;
  appLabel: string | null;
  versionName: string | null;
  versionCode: string | null;
  minSdk: string | null;
  targetSdk: string | null;
  apkPath: string | null;
  apkTotalSize: number | null;
  apkCount: number;
  iconPath: string | null;
  iconDataUrl: string | null;
  isSystem: boolean;
  isDebuggable: boolean;
};

export type ExportApksResult = {
  count: number;
  directory: string;
};

export const adbApkPaths = (device: string, packageName: string) =>
  invoke<string[]>('adb_apk_paths', { device, package: packageName });

export const adbPullApkForTool = (device: string, packageName: string, remotePath: string) =>
  invoke<string>('adb_pull_apk_for_tool', { device, package: packageName, remotePath });

export async function adbDevices(): Promise<AdbDevice[]> {
  return invoke<AdbDevice[]>('adb_devices');
}

export async function adbConnect(host: string, port: number): Promise<string> {
  return invoke<string>('adb_connect', { host, port });
}

export async function adbDisconnect(target?: string | null): Promise<string> {
  return invoke<string>('adb_disconnect', { target: target ?? null });
}

export async function adbListPackages(
  device: string,
  includeSystem: boolean,
): Promise<string[]> {
  return invoke<string[]>('adb_list_packages', { device, includeSystem });
}

export async function adbAppInfo(
  device: string,
  packageName: string,
): Promise<AppInfo> {
  return invoke<AppInfo>('adb_app_info', { device, package: packageName });
}

/**
 * Lightweight variant of `adbAppInfo` -- `dumpsys package` + `pm path` +
 * `wc`, no APK pull, no aapt2. ~5 IPCs instead of "pull a 30 MB APK".
 * Use this for per-card enrichment in the Apps tab; the heavy variant
 * stays available for callers that actually need the APK on disk.
 */
export async function adbAppInfoLite(
  device: string,
  packageName: string,
): Promise<AppInfo> {
  return invoke<AppInfo>('adb_app_info_lite', { device, package: packageName });
}

export async function adbListPackagesViaAgent(
  device: string,
): Promise<AppInfo[]> {
  return invoke<AppInfo[]>('adb_list_packages_via_agent', { device });
}

export async function adbAppIconViaAgent(
  device: string,
  packageName: string,
): Promise<string | null> {
  return invoke<string | null>('adb_app_icon_via_agent', { device, package: packageName });
}
export async function adbAppIcon(
  device: string,
  packageName: string,
): Promise<string | null> {
  return invoke<string | null>('adb_app_icon', { device, package: packageName });
}

export type InstallApkItemResult = { path: string; success: boolean; message: string };
export type InstallApksResult = { succeeded: number; failed: number; items: InstallApkItemResult[] };
export const adbInstallApks = (device: string, paths: string[]) =>
  invoke<InstallApksResult>('adb_install_apks', { device, paths });

export async function adbUninstall(
  device: string,
  packageName: string,
): Promise<string> {
  return invoke<string>('adb_uninstall', { device, package: packageName });
}

export async function adbExportApks(
  device: string,
  packageName: string,
  versionName: string | null,
  targetDir: string,
): Promise<ExportApksResult> {
  return invoke<ExportApksResult>('adb_export_apks', {
    device,
    package: packageName,
    versionName,
    targetDir,
  });
}

export async function adbLaunchApp(
  device: string,
  packageName: string,
): Promise<string> {
  return invoke<string>('adb_launch_app', { device, package: packageName });
}

/// Force-stop a running application. Idempotent: no error if it wasn't running.
export async function adbForceStop(
  device: string,
  packageName: string,
): Promise<string> {
  return invoke<string>('adb_force_stop', { device, package: packageName });
}

/// Clear an app's data + cache via `pm clear`. Note this wipes the app's
/// user data (permissions, accounts, settings) — not just cache.
export async function adbClearCache(
  device: string,
  packageName: string,
): Promise<string> {
  return invoke<string>('adb_clear_cache', { device, package: packageName });
}

/** Reboot `device`. `mode` is `null`/`undefined` for a normal reboot,
 *  `"recovery"` to drop into recovery, `"bootloader"` for fastboot. */
export async function adbReboot(
  device: string,
  mode?: 'recovery' | 'bootloader' | null,
): Promise<string> {
  return invoke<string>('adb_reboot', { device, mode: mode ?? null });
}

/** Power off `device`. Tries `adb reboot -p`; falls back to a power
 *  button keyevent on devices that lock down the syscall. */
export async function adbShutdown(device: string): Promise<string> {
  return invoke<string>('adb_shutdown', { device });
}

export type ShellOutput = {
  stdout: string;
  stderr: string;
  exitCode: number;
  command: string;
};

export async function adbShell(
  device: string,
  command: string,
): Promise<ShellOutput> {
  return invoke<ShellOutput>('adb_shell', { device, command });
}


export type DirEntry = {
  name: string;
  path: string;
  /** file | dir | link | other */
  kind: 'file' | 'dir' | 'link' | 'other';
  size: number;
  permissions: string;
  modified: string;
  linkTarget?: string | null;
};

/**
 * List a remote directory on the device.
 * @param asPkg When set, runs the listing via `run-as <pkg>` so a
 *   debuggable app's private dir can be browsed without root. When null,
 *   runs as the `shell` user (requires root or world-readable path).
 */
export async function listRemoteDir(
  device: string,
  path: string,
  asPkg?: string | null,
  useRoot = false,
): Promise<DirEntry[]> {
  return invoke<DirEntry[]>('list_remote_dir', {
    device,
    path,
    asPkg: asPkg ?? null,
    useRoot,
  });
}

export async function resolveAppDataDir(
  device: string,
  packageName: string,
  asPkg?: string | null,
  useRoot = false,
): Promise<string> {
  return invoke<string>('resolve_app_data_dir', {
    device,
    package: packageName,
    asPkg: asPkg ?? null,
    useRoot,
  });
}

/** Delete a file or directory on the device. `asPkg` follows the same
 *  semantics as `listRemoteDir`. */
export async function deleteRemoteFile(
  device: string,
  path: string,
  asPkg?: string | null,
  useRoot = false,
): Promise<string> {
  return invoke<string>('delete_remote_file', {
    device,
    path,
    asPkg: asPkg ?? null,
    useRoot,
  });
}

/**
 * Push a local file to a path on the device. Runs as the `shell` user, so
 * the target directory must be writable by `shell`.
 */
/**
 * Detect whether the connected device has root access (Magisk su, `adb root`,
 * or equivalent). Used to gate the release-package data-dir probe in
 * `AdbAppsTab.openDataDir`: a release package's private dir is only
 * accessible when the device is rooted.
 */
export async function isDeviceRooted(device: string): Promise<boolean> {
  return invoke<boolean>('is_device_rooted', { device });
}

export async function pullFile(
  device: string,
  remotePath: string,
  localPath: string,
  asPkg?: string | null,
  useRoot = false,
): Promise<string> {
  return invoke<string>('pull_file', {
    device,
    remotePath,
    localPath,
    asPkg: asPkg ?? null,
    useRoot,
  });
}

export async function screenshotPullToCache(device: string): Promise<string> {
  return invoke<string>('screenshot_pull_to_cache', { device });
}

export async function screenshotSaveFromCache(localPath: string): Promise<string> {
  return invoke<string>('screenshot_save_from_cache', { localPath });
}

export async function screenshotDiscardCache(): Promise<void> {
  return invoke<void>('screenshot_discard_cache');
}

export async function pushFile(
  device: string,
  localPath: string,
  remotePath: string,
  asPkg?: string | null,
  useRoot = false,
): Promise<string> {
  return invoke<string>('push_file', {
    device,
    localPath,
    remotePath,
    asPkg: asPkg ?? null,
    useRoot,
  });
}


/**
 * One-shot snapshot of device metadata for the System Info tab. Every
 * field is optional because each comes from a separate `adb shell`
 * call that may fail (locked SIM, missing battery service on
 * emulators, etc.). Missing fields should be rendered as em-dashes.
 */
export type DeviceSystemInfo = {
  // 硬件 / Hardware
  manufacturer?: string | null;
  brand?: string | null;
  model?: string | null;
  device?: string | null;
  hardware?: string | null;
  platform?: string | null;
  serial?: string | null;
  bootloader?: string | null;
  fingerprint?: string | null;

  // 屏幕 / Display
  screenSize?: string | null;
  screenDensity?: string | null;
  screenRefreshRate?: string | null;
  physicalSize?: string | null;
  rotation?: string | null;

  // 系统 / System
  androidRelease?: string | null;
  androidSdk?: string | null;
  securityPatch?: string | null;
  buildId?: string | null;
  buildType?: string | null;
  kernelVersion?: string | null;
  javaVm?: string | null;
  abi?: string | null;
  abiList?: string | null;

  // CPU
  cpuAbi?: string | null;
  cpuCores?: string | null;
  cpuHardware?: string | null;
  cpuMaxFreq?: string | null;
  cpuFeatures?: string | null;

  // GPU / 图形处理器
  gpuVendor?: string | null;
  gpuRenderer?: string | null;
  gpuOpenglesVersion?: string | null;
  gpuVulkanVersion?: string | null;
  gpuDriver?: string | null;

  // 内存 / Memory
  ramTotal?: string | null;
  ramAvailable?: string | null;

  // 存储 / Storage
  storageTotal?: string | null;
  storageAvailable?: string | null;

  // 网络 / Network
  wifiSsid?: string | null;
  wifiIp?: string | null;
  wifiSignal?: string | null;
  wifiLinkSpeed?: string | null;
  wifiFrequency?: string | null;
  networkType?: string | null;
  operator?: string | null;
  airplaneMode?: string | null;
  ipv4?: string | null;

  // 运行时 / Runtime
  uptime?: string | null;
  bootTime?: string | null;
  selinux?: string | null;
  timezone?: string | null;
  locale?: string | null;
  foregroundApp?: string | null;
  screenState?: string | null;

  // 电量 / Battery
  batteryLevel?: string | null;
  batteryStatus?: string | null;
  batteryHealth?: string | null;
  batteryTemp?: string | null;
  batteryVoltage?: string | null;
  batteryTechnology?: string | null;
  batteryPlugged?: string | null;
};

/**
 * Collect a snapshot of the connected device's hardware / system / screen /
 * CPU / network / battery metadata. Each section is queried
 * independently on the backend, so partial failures degrade gracefully —
 * a missing field should render as a placeholder rather than breaking
 * the whole tab.
 */
export async function adbSystemInfo(
  device: string,
): Promise<DeviceSystemInfo> {
  return invoke<DeviceSystemInfo>('adb_system_info', { device });
}

/**
 * Fast variant of `adbSystemInfo` -- asks the on-device agent to fill
 * the bulk of the fields via Binder (Build.* + SystemProperties ro.* +
 * ActivityManager + WifiManager + BatteryManager + Configuration), then
 * falls back to shell for anything the agent couldn't get.
 *
 * Falls back to the slow shell-only path on agent failure.
 */
export async function adbSystemInfoViaAgent(
  device: string,
): Promise<DeviceSystemInfo> {
  return invoke<DeviceSystemInfo>('adb_system_info_via_agent', { device });
}

// ---------------------------------------------------------------------------
// Logcat capture + download
// ---------------------------------------------------------------------------
// Logcat is intentionally NOT streamed live in this build: it pulls
// every line through the Tauri IPC bridge and re-renders the whole
// virtualized list per push, which costs both latency and battery. The
// workflow here is:
//   1. Capture (instant `-d` dump, or N-second follow) to a file on
//      /data/local/tmp/.
//   2. Pull that file down via the system save dialog and open it in
//      whatever external editor the user prefers.

/** Capture logcat on the device to `remotePath` and return line count.
 *
 *  `packageName` filters the stream to a single PID (resolved via
 *  `pidof`; returns NotFound from the backend when the package isn't
 *  running so the UI can prompt the user to launch the app first).
 *
 *  `durationSecs == 0` does an instant ring-buffer dump; `> 0` follows
 *  for that many seconds and is killed at the end. */
export async function adbLogcatCapture(
  device: string,
  packageName: string | null,
  remotePath: string,
  durationSecs: number,
): Promise<number> {
  return invoke<number>('adb_logcat_capture', {
    device,
    package: packageName,
    remotePath,
    durationSecs,
  });
}

/** Pull a previously-captured log file from the device to a local path
 *  (chosen via `dialog.save`). Returns the local path that was used. */
export async function adbLogcatPull(
  device: string,
  remotePath: string,
  localPath: string,
): Promise<string> {
  return invoke<string>('adb_logcat_pull', {
    device,
    remotePath,
    localPath,
  });
}

/**
 * Recovery variant detected by `adb_recovery_info`. The order in
 * the union matches the detection priority — a device that reports
 * `ro.twrp.version` is `twrp` regardless of what else it might
 * also report.
 */
export type RecoveryType = 'stock' | 'twrp' | 'orangefox' | 'lineageos' | 'aosp' | 'unknown';

/**
 * Per-device recovery diagnostics. Every field is best-effort —
 * stock recoveries often have no `ro.product.model` (the property
 * might be unset), custom recoveries sometimes strip
 * `ro.build.fingerprint`, etc. The UI renders missing fields as
 * em-dashes rather than as failures.
 */
export type RecoveryInfo = {
  recoveryType: RecoveryType;
  version: string | null;
  model: string | null;
  buildFingerprint: string | null;
  /** OEM manufacturer, e.g. `Xiaomi` / `Google` / `Samsung`. The UI
   *  combines this with `recoveryType` to render a brand-specific
   *  label like `Xiaomi 原厂 Recovery` without inflating the
   *  `recoveryType` enum. */
  manufacturer: string | null;
  /** Brand sub-classification, e.g. `Redmi` for Redmi-branded
   *  Xiaomi devices. Often equals the manufacturer for OEMs
   *  without sub-brands. */
  brand: string | null;
};

/**
 * Detect the recovery variant running on `device`. Runs a small set
 * of `getprop` probes — each is independent, so a single failure
 * doesn't poison the rest of the response. Stock recovery answers
 * most probes with empty / `<unknown>` and falls through to the
 * `stock` variant.
 */
export async function adbRecoveryInfo(device: string): Promise<RecoveryInfo> {
  return invoke<RecoveryInfo>('adb_recovery_info', { device });
}

/**
 * Run `adb sideload <path>` on `device`. Verifies the path is a real
 * file first (the Rust side double-checks) so a stale file picker
 * selection doesn't reach the device. Returns the raw `adb` stdout
 * — typically `Total xfer: 1.00x` on success or a multi-line error
 * on failure.
 */
export async function adbSideload(device: string, path: string): Promise<string> {
  return invoke<string>('adb_sideload', { device, path });
}

/**
 * Run `adb -s <serial> tcpip <port>` -- ask the on-device adbd to
 * listen on `<port>` so the workstation can `adb connect <ip>:<port>`
 * without USB. Pre-Android-11 wireless-debugging entry point; still
 * works on Android 11+ and is the path of least resistance when both
 * devices are on the same Wi-Fi.
 *
 * Returns trimmed `adb` stdout (typically `restarting in TCP mode
 * port: 5555`). Caller surfaces the IP separately.
 */
export async function adbTcpip(device: string, port: number): Promise<string> {
  return invoke<string>('adb_tcpip', { device, port });
}

/**
 * Run `adb -s <serial> reconnect` -- drop and re-establish the
 * connection to a device stuck in `offline` / `unauthorized`.
 * Useful right after `tcpip` toggling or after the user re-grants
 * USB-debugging auth on the device.
 */
export async function adbReconnect(device: string): Promise<string> {
  return invoke<string>('adb_reconnect', { device });
}
