import { check as pluginCheck, type CheckOptions, type DownloadEvent, type Update } from '@tauri-apps/plugin-updater';
import { getVersion as tauriGetVersion } from '@tauri-apps/api/app';

export const UPDATE_CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;
const LAST_CHECK_KEY = 'jadb-update-last-check';

export type UpdateInfo = {
  version: string;
  notes?: string | undefined;
  pubDate?: string | undefined;
};

let currentUpdate: Update | null = null;

function recentlyChecked(): boolean {
  const value = localStorage.getItem(LAST_CHECK_KEY);
  if (!value) return false;
  const timestamp = Date.parse(value);
  return Number.isFinite(timestamp) && Date.now() - timestamp < UPDATE_CHECK_INTERVAL_MS;
}

export function markChecked(): void {
  localStorage.setItem(LAST_CHECK_KEY, new Date().toISOString());
}

export async function checkForUpdate(options?: CheckOptions, force = false): Promise<UpdateInfo | null> {
  if (!force && recentlyChecked()) return null;
  const update = await pluginCheck(options);
  markChecked();
  currentUpdate = update;
  return update
    ? { version: update.version, notes: update.body ?? undefined, pubDate: update.date ?? undefined }
    : null;
}

export async function downloadAndInstall(onProgress?: (progress: number) => void): Promise<void> {
  if (!currentUpdate) await checkForUpdate({ timeout: 5000 }, true);
  if (!currentUpdate) throw new Error('No update available');

  let downloaded = 0;
  await currentUpdate.downloadAndInstall((event: DownloadEvent) => {
    if (event.event === 'Started') {
      downloaded = 0;
      onProgress?.(0);
    } else if (event.event === 'Progress') {
      downloaded += event.data.chunkLength;
      // Tauri does not always expose the total size, so keep this indeterminate
      // progress below 100% until the Finished event arrives.
      onProgress?.(Math.min(99, Math.max(1, downloaded % 99)));
    } else if (event.event === 'Finished') {
      onProgress?.(100);
    }
  });
  await currentUpdate.close();
  currentUpdate = null;
}

export function getLastCheckTime(): string | null {
  return localStorage.getItem(LAST_CHECK_KEY);
}

export function getCurrentVersion(): Promise<string> {
  return tauriGetVersion();
}
