import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type ToolName =
  | 'apktool'
  | 'uber-apk-signer'
  | 'android-build-tools'
  | 'jadx'
  | 'aapt2'
  | 'adb'
  | 'java';

export type ToolSource = 'bundled' | 'local' | 'fallback';

export type ToolStatus = {
  name: ToolName;
  installed: boolean;
  version: string | null;
  path: string | null;
  downloadUrl: string | null;
  source: ToolSource;
};

export type InstallProgress = {
  name: string;
  downloaded: number;
  total: number;
};

export async function getToolStatus(): Promise<ToolStatus[]> {
  return invoke<ToolStatus[]>('get_tool_status');
}

export async function installTool(name: ToolName): Promise<ToolStatus> {
  return invoke<ToolStatus>('install_tool', { name });
}

export async function removeTool(name: ToolName): Promise<void> {
  return invoke<void>('remove_tool', { name });
}

export function onInstallProgress(cb: (p: InstallProgress) => void): Promise<UnlistenFn> {
  return listen<InstallProgress>('tool://install-progress', (e) => cb(e.payload));
}
