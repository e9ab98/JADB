import { invoke } from '@tauri-apps/api/core';
import { open as openDialog } from '@tauri-apps/plugin-dialog';

export async function fileSize(path: string): Promise<number> {
  return invoke<number>('file_size', { path });
}

export async function pickDirectory(title?: string): Promise<string | null> {
  const picked = await openDialog({
    multiple: false,
    directory: true,
    ...(title ? { title } : {}),
  });
  return typeof picked === 'string' ? picked : null;
}

export async function getLogPath(): Promise<string> {
  return invoke<string>('get_log_path');
}
