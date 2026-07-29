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
