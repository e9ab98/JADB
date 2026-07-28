import { invoke } from '@tauri-apps/api/core';

/**
 * Open (or focus) the apps window for a given adb device serial.
 *
 * One window is created per serial; calling this again with the same serial
 * simply focuses the existing window. Returns when the window has been
 * created (or focused) on the OS.
 */
export async function openAppsWindow(serial: string): Promise<void> {
  await invoke<void>('open_apps_window', { serial });
}

/**
 * Open (or focus) the per-app data-dir file manager window for a given
 * device + package. One window per (device, pkg) pair.
 */
export async function openDataDirWindow(
  device: string,
  pkg: string,
  debuggable: boolean,
  useRoot: boolean,
  rootPath: string,
): Promise<void> {
  await invoke<void>('open_data_dir_window', {
    device,
    pkg,
    debuggable,
    useRoot,
    rootPath,
  });
}

/**
 * Open (or focus) the standalone 反编译 window. Lives outside the main
 * sidebar so the user can keep working in the shell while a decompile
 * task runs.
 */
export async function openDecompileWindow(): Promise<void> {
  await invoke<void>('open_decompile_window');
}

/**
 * Open (or focus) the standalone 重打包 window.
 */
export async function openRepackageWindow(): Promise<void> {
  await invoke<void>('open_repackage_window');
}

/**
 * Open (or focus) the standalone 分析 window.
 */
export async function openAnalyzeWindow(): Promise<void> {
  await invoke<void>('open_analyze_window');
}
