import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import type { TaskHandle } from '@/ipc/types';


export async function pickApkFile(): Promise<string | null> {
  const picked = await open({
    multiple: false,
    filters: [{ name: 'APK', extensions: ['apk'] }],
  });
  return typeof picked === 'string' ? picked : null;
}

export async function pickOutDir(): Promise<string | null> {
  const picked = await open({ multiple: false, directory: true });
  return typeof picked === 'string' ? picked : null;
}

/**
 * Resolve a non-conflicting output directory inside the picked parent,
 * named after the APK. Used to work around apktool refusing to write
 * into any pre-existing directory unless `-f` is passed.
 */
export async function resolveUniqueOutDir(parent: string, baseName: string): Promise<string> {
  return invoke<string>('resolve_unique_out_dir', { parent, baseName });
}

export async function decompileApk(path: string, outDir: string, force: boolean): Promise<TaskHandle> {
  return invoke<TaskHandle>('decompile_apk', { path, outDir, force });
}

export async function openPath(path: string): Promise<void> {
  return invoke<void>('open_path', { path });
}
