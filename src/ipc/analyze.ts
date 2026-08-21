import { invoke } from '@tauri-apps/api/core';
import type { PackerReport, SignatureInfo } from '@/types/signing';

export type VolumeCategory = {
  dex: number;
  lib: number;
  res: number;
  assets: number;
  manifest: number;
  arsc: number;
  other: number;
};

export type RedundantFile = {
  crc: string;
  size: number;
  files: string[];
};

export type VolumeStats = {
  dex: number;
  lib: number;
  res: number;
  assets: number;
  manifest: number;
  arsc: number;
  other: number;
  lib_breakdown: Record<string, number>;
  redundant_files: RedundantFile[];
  waste_size: number;
};

export type VolumeEntry = {
  name: string;
  size: number;
  ratio: number;
};

export type SecurityRisk = {
  id: string;
  level: 'critical' | 'warning' | 'info' | string;
  title: string;
  description: string;
  suggestion: string;
};

export type SecurityReport = {
  risks: SecurityRisk[];
  /** 0..=100. */
  score: number;
};

export type ApkInfo = {
  package_name: string;
  version_code: string | null;
  version_name: string | null;
  min_sdk: string | null;
  target_sdk: string | null;
  max_sdk: string | null;
  application_label: string | null;
  permissions: string[];
  activities: string[];
  services: string[];
  receivers: string[];
  providers: string[];
  raw_badging: string;
  native_libs?: string[];
  intent_actions?: string[];
  /** Hardware / software features the app needs (e.g. `android.hardware.camera`). */
  uses_feature?: string[];
  /** Shared libraries the app links to via `<uses-library>`. */
  uses_library?: string[];
  /** Runtime-only permissions (Android 6.0+). */
  uses_permission_sdk_23?: string[];
  /** Screen size / density buckets the app actively supports. */
  supports_screens?: string[];
  /** BCP-47 language tags the app ships localised resources for. */
  locales?: string[];
  /** `application-debuggable` line from badging; surfaces above the
   *  security report so the basicInfo card can render it without
   *  traversing the nested structure. */
  application_debuggable?: boolean;
  /** Fraction of class names whose last segment is <= 3 chars (a
   *  ProGuard / R8 obfuscation heuristic). Defaults to 0.0 for
   *  tiny APKs that fall below the heuristic's `total < 4` floor. */
  short_name_ratio?: number;
  file_size?: number | null;
  volume_total_size?: number | null;
  volume_stats?: VolumeStats | null;
  largest_files?: VolumeEntry[];
  insights?: string[];
  native_libraries?: Record<string, string[]>;
  tech_stack?: string[];
  security_report?: SecurityReport | null;
  /** Per-signer certificate detail from `apksigner verify --print-certs`.
   *  Present when the analysis pipeline was able to invoke apksigner;
   *  missing when the tool is not configured or the call failed. */
  signature?: SignatureInfo | null;
  /** Heuristic packer / shell / obfuscator detection result. Computed by
   *  zip-path signature matching during analysis; cheap to compute and
   *  always present unless the analyze pipeline itself failed. */
  packer?: PackerReport | null;
  /** Absolute path of the launcher icon inside the APK zip, as
   *  reported by aapt2 `dump badging`. Always paired with
   *  `iconDataUrl` when present; either both set or both null so
   *  callers never have to handle "path without bytes". Useful
   *  for debug overlays that want to show the raw archive path. */
  iconPath?: string | null;
  /** `data:image/png|webp|jpeg;base64,...` for the launcher icon.
   *  Read from the APK zip on the Rust side because the frontend
   *  cannot reach inside the archive. `null` when the APK has no
   *  raster `application-icon-*` line — the UI falls back to a
   *  letter avatar in that case. Adaptive-icon XML descriptors
   *  are intentionally skipped (single-image rendering). */
  iconDataUrl?: string | null;
};

export async function analyzeApk(path: string): Promise<ApkInfo> {
  return invoke<ApkInfo>('analyze_apk', { path });
}
