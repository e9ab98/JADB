import { invoke } from '@tauri-apps/api/core';
import { save } from '@tauri-apps/plugin-dialog';
import type { TaskHandle } from '@/ipc/types';
import type { SigningSchemes } from '@/types/signing';


export async function pickSrcDir(): Promise<string | null> {
  const { open } = await import('@tauri-apps/plugin-dialog');
  const picked = await open({ multiple: false, directory: true });
  return typeof picked === 'string' ? picked : null;
}

export async function pickOutApk(): Promise<string | null> {
  const picked = await save({
    filters: [{ name: 'APK', extensions: ['apk'] }],
    defaultPath: 'repackaged.apk',
  });
  return typeof picked === 'string' ? picked : null;
}

export async function repackageApk(
  srcDir: string,
  outApk: string,
  sign: boolean,
  signatureId: string | null,
  schemes: SigningSchemes,
): Promise<TaskHandle> {
  return invoke<TaskHandle>('repackage_apk', {
    srcDir,
    outApk,
    sign,
    signatureId,
    schemes,
  });
}
