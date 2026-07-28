import { invoke } from '@tauri-apps/api/core';

export type CacheFileEntry = {
  category: 'app_cache' | 'temp_dir' | string;
  path: string;
  bytes: number;
};

export type CacheCategorySummary = {
  id: 'app_cache' | 'temp_dir' | string;
  label: string;
  bytes: number;
  fileCount: number;
};

export type CacheScanResult = {
  totalBytes: number;
  totalFiles: number;
  categories: CacheCategorySummary[];
  items: CacheFileEntry[];
  /** True when the preview list was truncated to keep the dialog snappy. */
  truncated: boolean;
};

export type CacheDeleteError = {
  path: string;
  reason: string;
};

export type CacheClearResult = {
  deletedFiles: number;
  deletedBytes: number;
  errors: CacheDeleteError[];
};

/** Walk the on-disk caches and return their aggregate size + preview list. */
export async function scanCache(): Promise<CacheScanResult> {
  return invoke<CacheScanResult>('scan_cache');
}

/** Delete everything `scan_cache` would have shown. Re-walks at delete
 *  time so any cache entries added between scan and confirm also go. */
export async function clearCache(): Promise<CacheClearResult> {
  return invoke<CacheClearResult>('clear_cache');
}
