import { create } from 'zustand';
import {
  activateLicense,
  getLicenseServerUrl,
  getLicenseStatus,
  refreshLicenseStatus,
  removeLicense,
  replaceLicenseBinding,
  verifyLicenseRemote,
  type LicenseFeature,
  type LicenseStatus,
} from '@/ipc/license';

type LicenseStore = {
  status: LicenseStatus | null;
  loading: boolean;
  error: string | null;
  promptFeature: LicenseFeature | null;
  serverUrl: string | null;
  refresh: () => Promise<void>;
  activate: (token: string) => Promise<void>;
  remove: () => Promise<void>;
  refreshRemote: () => Promise<void>;
  verifyRemote: (token: string) => Promise<LicenseStatus>;
  /**
   * 在线模式「替换绑定」：把当前 license 的 server 端绑定替换到本机。
   * 调用前应已激活过 license（不然后端会报错）。
   */
  replaceBinding: () => Promise<void>;
  loadServerUrl: () => Promise<void>;
  hasFeature: (feature: LicenseFeature) => boolean;
  requireFeature: (feature: LicenseFeature) => boolean;
  closePrompt: () => void;
};

export const useLicenseStore = create<LicenseStore>((set, get) => ({
  status: null,
  loading: false,
  error: null,
  promptFeature: null,
  serverUrl: null,

  async refresh() {
    set({ loading: true, error: null });
    try {
      set({ status: await getLicenseStatus(), loading: false });
    } catch (error) {
      set({ error: String(error), loading: false });
    }
  },

  async activate(token) {
    set({ loading: true, error: null });
    try {
      set({ status: await activateLicense(token), loading: false, promptFeature: null });
    } catch (error) {
      set({ error: String(error), loading: false });
      throw error;
    }
  },

  async remove() {
    set({ loading: true, error: null });
    try {
      set({ status: await removeLicense(), loading: false });
    } catch (error) {
      set({ error: String(error), loading: false });
      throw error;
    }
  },

  async refreshRemote() {
    set({ loading: true, error: null });
    try {
      set({ status: await refreshLicenseStatus(), loading: false });
    } catch (error) {
      set({ error: String(error), loading: false });
      throw error;
    }
  },

  async verifyRemote(token) {
    return await verifyLicenseRemote(token);
  },

  async replaceBinding() {
    set({ loading: true, error: null });
    try {
      set({ status: await replaceLicenseBinding(), loading: false });
    } catch (error) {
      set({ error: String(error), loading: false });
      throw error;
    }
  },

  async loadServerUrl() {
    try {
      set({ serverUrl: await getLicenseServerUrl() });
    } catch (error) {
      // 静默：server URL 读不到就走当前缓存值
      console.warn('loadServerUrl failed:', error);
    }
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
