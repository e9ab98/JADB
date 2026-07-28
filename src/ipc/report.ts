import { invoke } from '@tauri-apps/api/core';

export type ExportReportArgs = {
  dest_path: string;
  html: string;
};

export type ExportReportResult = {
  dest_path: string;
  bytes_written: number;
};

export async function exportApkReport(args: ExportReportArgs): Promise<ExportReportResult> {
  return invoke<ExportReportResult>('export_apk_report', { args });
}
