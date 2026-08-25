import {
  check as pluginCheck,
  type CheckOptions,
  type DownloadEvent,
  type Update,
} from '@tauri-apps/plugin-updater';
import { getVersion as tauriGetVersion } from '@tauri-apps/api/app';

export const LS_KEYS = {
  checked: 'jadb-update-checked',
  dismissed: 'jadb-update-dismissed',
  notifyOnUpdate: 'jadb-notify-on-update',
} as const;

export const UPDATE_CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;

export type UpdateInfo = {
  version: string;
  notes?: string | undefined;
  pubDate?: string | undefined;
};

let lastUpdate: UpdateInfo | null = null;
let lastUpdateHandle: Update | null = null;

function within24h(): boolean {
  const raw = localStorage.getItem(LS_KEYS.checked);
  if (!raw) return false;
  const t = Date.parse(raw);
  if (Number.isNaN(t)) return false;
  return Date.now() - t < UPDATE_CHECK_INTERVAL_MS;
}

/**
 * Compare two dotted version strings (e.g. "0.3.10" vs "0.3.9").
 * Returns true when `latest` is strictly greater than `current`.
 *
 * Pre-release suffixes (e.g. "0.4.0-rc.1") are not supported: the
 * trailing `parseInt` yields NaN and the comparison falls back to
 * equal, which silently treats "0.4.0-rc.1" as not-newer than "0.3.9".
 */
export function isNewer(latest: string, current: string): boolean {
  const a = latest.split('.').map((n) => parseInt(n, 10));
  const b = current.split('.').map((n) => parseInt(n, 10));
  const len = Math.max(a.length, b.length);
  for (let i = 0; i < len; i++) {
    const av = Number.isFinite(a[i]) ? (a[i] as number) : 0;
    const bv = Number.isFinite(b[i]) ? (b[i] as number) : 0;
    if (av > bv) return true;
    if (av < bv) return false;
  }
  return false;
}

export function shouldNotify(): boolean {
  return localStorage.getItem(LS_KEYS.notifyOnUpdate) !== 'false';
}

export function setNotify(enabled: boolean): void {
  localStorage.setItem(LS_KEYS.notifyOnUpdate, enabled ? 'true' : 'false');
}

export function markChecked(): void {
  localStorage.setItem(LS_KEYS.checked, new Date().toISOString());
}

export function markDismissed(version: string): void {
  localStorage.setItem(LS_KEYS.dismissed, version);
}

export function wasDismissed(version: string): boolean {
  return localStorage.getItem(LS_KEYS.dismissed) === version;
}

/**
 * Check for an available update.
 *
 * - `force=false` (default): if a check ran within the last 24h,
 *   return the cached `lastUpdate` (which may be a previous UpdateInfo
 *   or null). Otherwise hit the network.
 * - `force=true`: always hit the network and refresh the cache.
 *
 * The caller is expected to read `currentUpdate()` or rely on the
 * returned info; `lastUpdateHandle` is populated for `downloadAndInstall`.
 */
export async function checkForUpdate(
  options?: CheckOptions,
  force = false,
): Promise<UpdateInfo | null> {
  if (!force && within24h()) return lastUpdate;
  const r = await pluginCheck(options as Parameters<typeof pluginCheck>[0]);
  markChecked();
  if (!r) {
    lastUpdate = null;
    lastUpdateHandle = null;
    return null;
  }
  lastUpdateHandle = r;
  lastUpdate = {
    version: r.version,
    notes: r.body ?? undefined,
    pubDate: r.date ?? undefined,
  };
  return lastUpdate;
}

export async function downloadAndInstall(
  onProgress?: (progress: number) => void,
): Promise<void> {
  // Re-check if we don't have a live handle (caller invoked without check()).
  if (!lastUpdateHandle) {
    await checkForUpdate();
  }
  if (!lastUpdateHandle) {
    throw new Error('No update available');
  }

  let downloaded = 0;
  await lastUpdateHandle.downloadAndInstall((event: DownloadEvent) => {
    if (event.event === 'Started') {
      downloaded = 0;
      onProgress?.(0);
    } else if (event.event === 'Progress') {
      downloaded += event.data.chunkLength;
      // Tauri does not always expose the total size, so keep this
      // indeterminate progress below 100% until the Finished event arrives.
      onProgress?.(Math.min(99, Math.max(1, downloaded % 99)));
    } else if (event.event === 'Finished') {
      onProgress?.(100);
    }
  });
  // Best-effort cleanup of the underlying update resource.
  try {
    await lastUpdateHandle.close();
  } catch {
    // ignore
  }
  lastUpdateHandle = null;
  // Keep `lastUpdate` populated so the Settings tab can still show
  // "newest available" after a download completes.
}

export function getLastCheckTime(): string | null {
  return localStorage.getItem(LS_KEYS.checked);
}

export function getCurrentVersion(): Promise<string> {
  return tauriGetVersion();
}
