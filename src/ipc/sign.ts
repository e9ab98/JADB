import { invoke } from '@tauri-apps/api/core';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import type { TaskHandle } from '@/ipc/types';
import type {
  SignatureInfo,
  SigningSchemes,
} from '@/types/signing';

export type SignRequest =
  | {
      mode: 'standard';
      apkPath: string;
      signatureId: string;
      allowResign: boolean;
      schemes: SigningSchemes;
    }
  | {
      mode: 'rotation';
      apkPath: string;
      lineageId: string;
      allowResign: boolean;
      v4Enabled: boolean;
    };

export async function checkApkSigned(apkPath: string): Promise<boolean> {
  return invoke<boolean>('check_apk_signed', { apkPath });
}

export async function signApk(request: SignRequest): Promise<TaskHandle> {
  return invoke<TaskHandle>('sign_apk', { request });
}

export async function inspectSignature(apkPath: string): Promise<SignatureInfo> {
  return invoke<SignatureInfo>('inspect_signature', { apkPath });
}

export async function stripApkSigning(
  apkPath: string,
  outputPath?: string | null,
): Promise<TaskHandle> {
  return invoke<TaskHandle>('strip_apk_signing', { apkPath, outputPath });
}

export async function pickApk(): Promise<string | null> {
  const p = await openDialog({ multiple: false, filters: [{ name: 'APK', extensions: ['apk'] }] });
  return typeof p === 'string' ? p : null;
}

export async function pickStripOutputDir(): Promise<string | null> {
  const p = await openDialog({
    multiple: false,
    directory: true,
  });
  return typeof p === 'string' ? p : null;
}
