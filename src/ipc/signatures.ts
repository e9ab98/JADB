import { invoke } from '@tauri-apps/api/core';
import { save as saveDialog } from '@tauri-apps/plugin-dialog';

export type SignatureConfig = {
  id: string;
  label: string;
  keystorePath: string;
  keystorePassword: string;
  keyAlias: string;
  keyPassword: string;
  createdAt: string;
};

export type NewKeystoreDName = {
  cn?: string;
  ou?: string;
  o?: string;
  l?: string;
  st?: string;
  c?: string;
};

export type NewKeystoreOptions = {
  keyAlgorithm?: 'RSA' | 'EC';
  keySize?: number;
  validityDays?: number;
  dname?: NewKeystoreDName;
};

export type NewKeystoreInput = {
  label: string;
  alias: string;
  keystorePassword: string;
  keyPassword: string;
  options?: NewKeystoreOptions;
};

export const KEY_ALGORITHM_OPTIONS = ['RSA', 'EC'] as const;

export const KEY_SIZE_OPTIONS: Record<'RSA' | 'EC', number[]> = {
  RSA: [2048, 3072, 4096],
  EC: [256, 384, 521],
};

export async function listSignatures(): Promise<SignatureConfig[]> {
  return invoke<SignatureConfig[]>('list_signatures');
}

export async function createNewKeystore(
  input: NewKeystoreInput,
): Promise<SignatureConfig> {
  return invoke<SignatureConfig>('create_new_keystore', { input });
}

export async function exportSignature(id: string, destPath: string): Promise<string> {
  return invoke<string>('export_signature', { id, destPath });
}

export async function pickSignatureExportPath(
  defaultName: string,
): Promise<string | null> {
  const p = await saveDialog({
    defaultPath: defaultName,
    filters: [{ name: 'Keystore', extensions: ['jks', 'keystore'] }],
  });
  return typeof p === 'string' ? p : null;
}

export async function updateSignature(
  id: string,
  patch: Partial<SignatureConfig>,
): Promise<SignatureConfig> {
  return invoke<SignatureConfig>('update_signature', { id, patch });
}

export async function deleteSignature(id: string): Promise<void> {
  return invoke<void>('delete_signature', { id });
}

export async function importKeystore(
  srcPath: string,
  alias: string,
  password: string,
  label: string,
): Promise<SignatureConfig> {
  return invoke<SignatureConfig>('import_keystore', { srcPath, alias, password, label });
}
