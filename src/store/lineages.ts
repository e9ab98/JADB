import { create } from 'zustand';
import {
  listLineages,
  createLineage,
  importLineage,
  deleteLineage,
  exportLineage,
  type CreateLineageInput,
  type ImportLineageInput,
} from '@/ipc/lineages';
import type { LineageConfig, LineageStatus } from '@/types/lineage';

type LineagesState = {
  list: LineageStatus[];
  loading: boolean;
  refresh: () => Promise<void>;
  create: (input: CreateLineageInput) => Promise<LineageConfig>;
  import: (input: ImportLineageInput) => Promise<LineageConfig>;
  remove: (id: string) => Promise<void>;
  export: (id: string, destPath: string) => Promise<string>;
};

export const useLineagesStore = create<LineagesState>((set, get) => ({
  list: [],
  loading: false,
  async refresh() {
    set({ loading: true });
    try {
      set({ list: await listLineages(), loading: false });
    } catch (e) {
      set({ loading: false });
      throw e;
    }
  },
  async create(input) {
    const created = await createLineage(input);
    await get().refresh();
    return created;
  },
  async import(input) {
    const created = await importLineage(input);
    await get().refresh();
    return created;
  },
  async remove(id) {
    await deleteLineage(id);
    await get().refresh();
  },
  async export(id, destPath) {
    return exportLineage(id, destPath);
  },
}));
