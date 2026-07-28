import { create } from 'zustand';
import {
  listSignatures,
  createNewKeystore,
  exportSignature,
  updateSignature,
  deleteSignature,
  importKeystore,
  pickSignatureExportPath,
  type SignatureConfig,
  type NewKeystoreInput,
} from '@/ipc/signatures';

export type CreateSignatureInput = NewKeystoreInput;

type SignaturesState = {
  list: SignatureConfig[];
  loading: boolean;
  refresh: () => Promise<void>;
  create: (input: CreateSignatureInput) => Promise<SignatureConfig>;
  update: (id: string, patch: Partial<SignatureConfig>) => Promise<void>;
  remove: (id: string) => Promise<void>;
  import: (
    srcPath: string,
    alias: string,
    password: string,
    label: string,
  ) => Promise<SignatureConfig>;
  exportKeystore: (id: string, defaultName: string) => Promise<string | null>;
};

export const useSignaturesStore = create<SignaturesState>((set, get) => ({
  list: [],
  loading: false,
  async refresh() {
    set({ loading: true });
    try {
      set({ list: await listSignatures(), loading: false });
    } catch (e) {
      set({ loading: false });
      throw e;
    }
  },
  async create(input) {
    const created = await createNewKeystore(input);
    await get().refresh();
    return created;
  },
  async update(id, patch) {
    const existing = get().list.find((s) => s.id === id);
    if (!existing) throw new Error(`signature not found: ${id}`);
    const full = { ...existing, ...patch };
    await updateSignature(id, full);
    await get().refresh();
  },
  async remove(id) {
    await deleteSignature(id);
    await get().refresh();
  },
  async import(srcPath, alias, password, label) {
    const created = await importKeystore(srcPath, alias, password, label);
    await get().refresh();
    return created;
  },
  async exportKeystore(id, defaultName) {
    const dest = await pickSignatureExportPath(defaultName);
    if (!dest) return null;
    const written = await exportSignature(id, dest);
    await get().refresh();
    return written;
  },
}));
