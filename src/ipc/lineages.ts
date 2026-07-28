import { invoke } from '@tauri-apps/api/core';
import { save as saveDialog } from '@tauri-apps/plugin-dialog';
import type { LineageConfig, LineageStatus } from '@/types/lineage';

export async function listLineages(): Promise<LineageStatus[]> {
  return invoke<LineageStatus[]>('list_lineages');
}

export type CreateLineageInput = {
  label: string;
  oldSignatureId: string;
  newSignatureId: string;
};

export async function createLineage(
  input: CreateLineageInput,
): Promise<LineageConfig> {
  return invoke<LineageConfig>('create_lineage', { input });
}

export type ImportLineageInput = {
  label: string;
  srcPath: string;
  oldSignatureId: string;
  newSignatureId: string;
};

export async function importLineage(
  input: ImportLineageInput,
): Promise<LineageConfig> {
  return invoke<LineageConfig>('import_lineage', { input });
}

export async function deleteLineage(id: string): Promise<void> {
  return invoke<void>('delete_lineage', { id });
}

export async function exportLineage(id: string, destPath: string): Promise<string> {
  return invoke<string>('export_lineage', { id, destPath });
}

export async function pickLineageExportPath(defaultName: string): Promise<string | null> {
  const p = await saveDialog({
    defaultPath: defaultName,
    filters: [{ name: 'Lineage', extensions: ['lineage'] }],
  });
  return typeof p === 'string' ? p : null;
}

