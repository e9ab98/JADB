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
