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
};

export async function analyzeApk(path: string): Promise<ApkInfo> {
  return invoke<ApkInfo>('analyze_apk', { path });
}
