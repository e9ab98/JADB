import { invoke } from '@tauri-apps/api/core';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import type { TaskHandle } from '@/ipc/types';


export type JadxOptions = {
  showInGradle: boolean;
  decompileResources: boolean;
  debugInfo: boolean;
  exportAsGradle: boolean;
  threadsCount: number | null;
};

export async function jadxDecompile(
  path: string,
  outDir: string,
  options: JadxOptions,
): Promise<TaskHandle> {
  return invoke<TaskHandle>('jadx_decompile', { path, outDir, options });
}

/**
 * Launch the JADX GUI in a detached child process. The binary must be
 * configured in Settings → Tools (path must contain `bin/jadx-gui`,
 * or `bin/jadx-gui.bat` on Windows). Throws via the Tauri error pipe if
 * the binary is missing or not executable.
 *
 * The Rust side sets `JADX_GUI_OPTS=-Dfile.encoding=UTF-8` (plus
 * `JAVA_OPTS` / `JAVA_TOOL_OPTIONS` as fallbacks) so non-ASCII paths
 * (CJK / emoji / etc.) are decoded correctly in JADX-GUI's UI and
 * file dialog.
 */
export async function launchJadxGui(): Promise<void> {
  await invoke<void>('launch_jadx_gui');
}

export async function pickApk(): Promise<string | null> {
  const p = await openDialog({
    multiple: false,
    filters: [{ name: 'APK', extensions: ['apk'] }],
  });
  return typeof p === 'string' ? p : null;
}

export async function pickOutDir(): Promise<string | null> {
  const p = await openDialog({ multiple: false, directory: true });
  return typeof p === 'string' ? p : null;
}
