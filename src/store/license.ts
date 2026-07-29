import { create } from 'zustand';
import {
  activateLicense,
  getLicenseStatus,
  removeLicense,
  type LicenseFeature,
  type LicenseStatus,
} from '@/ipc/license';

type LicenseStore = {
  status: LicenseStatus | null;
  loading: boolean;
  error: string | null;
  promptFeature: LicenseFeature | null;
  refresh: () => Promise<void>;
  activate: (token: string) => Promise<void>;
  remove: () => Promise<void>;
  hasFeature: (feature: LicenseFeature) => boolean;
  requireFeature: (feature: LicenseFeature) => boolean;
  closePrompt: () => void;
};

export const useLicenseStore = create<LicenseStore>((set, get) => ({
  status: null,
  loading: false,
  error: null,
  promptFeature: null,
  async refresh() {
    set({ loading: true, error: null });
    try { set({ status: await getLicenseStatus(), loading: false }); }
    catch (error) { set({ error: String(error), loading: false }); }
  },
  async activate(token) {
    set({ loading: true, error: null });
    try { set({ status: await activateLicense(token), loading: false, promptFeature: null }); }
    catch (error) { set({ error: String(error), loading: false }); throw error; }
  },
  async remove() {
    set({ loading: true, error: null });
    try { set({ status: await removeLicense(), loading: false }); }
    catch (error) { set({ error: String(error), loading: false }); throw error; }
  },
  hasFeature(feature) {
    const status = get().status;
    if (status?.state !== 'active') return false;
    if (status.features.includes('all')) return true;
    return status.features.includes(feature);
  },
  requireFeature(feature) {
    if (get().hasFeature(feature)) return true;
    set({ promptFeature: feature });
    return false;
  },
  closePrompt: () => set({ promptFeature: null }),
}));
