import { invoke } from '@tauri-apps/api/core';

export type Language = 'zhcn' | 'en';
export type ThemeMode = 'system' | 'light' | 'dark';

export type Settings = {
  aaptPath: string | null;
  adbPath: string | null;
  apktoolPath: string | null;
  uberApkSignerPath: string | null;
  apksignerPath: string | null;
  androidBuildToolsDir: string | null;
  jadxDir: string | null;
  javaDir: string | null;
  rulesPath: string | null;
  rulesDownloadUrl: string | null;
  licenseServerUrl?: string | null;
  language: Language;
  theme: ThemeMode;
};

export type SettingsPatch = {
  aaptPath?: string | null | undefined;
  adbPath?: string | null | undefined;
  apktoolPath?: string | null | undefined;
  uberApkSignerPath?: string | null | undefined;
  apksignerPath?: string | null | undefined;
  androidBuildToolsDir?: string | null | undefined;
  jadxDir?: string | null | undefined;
  javaDir?: string | null | undefined;
  rulesPath?: string | null | undefined;
  rulesDownloadUrl?: string | null | undefined;
  licenseServerUrl?: string | null | undefined;
  language?: Language;
  theme?: ThemeMode;
};

export async function getSettings(): Promise<Settings> {
  return invoke<Settings>('get_settings');
}

export async function updateSettings(patch: SettingsPatch): Promise<Settings> {
  return invoke<Settings>('update_settings', { patch });
}
