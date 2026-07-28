import { create } from 'zustand';
import {
  getSettings,
  updateSettings,
  type Settings,
  type SettingsPatch,
  type Language,
  type ThemeMode,
} from '@/ipc/useTauri';
import { onSettingsChanged } from '@/ipc/events';

type SettingsState = {
  settings: Settings | null;
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  setLanguage: (lang: Language) => Promise<void>;
  setTheme: (mode: ThemeMode) => Promise<void>;
  patch: (p: SettingsPatch) => Promise<void>;
};

export const useSettingsStore = create<SettingsState>((set) => ({
  settings: null,
  loading: false,
  error: null,
  async refresh() {
    set({ loading: true, error: null });
    try {
      const settings = await getSettings();
      set({ settings, loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },
  async setLanguage(lang) {
    const next = await updateSettings({ language: lang });
    set({ settings: next });
  },
  async setTheme(mode) {
    const next = await updateSettings({ theme: mode });
    set({ settings: next });
  },
  async patch(p) {
    const next = await updateSettings(p);
    set({ settings: next });
  },
}));

// Side-effect: subscribe to backend-side changes and mirror them in the store.
onSettingsChanged((settings) => useSettingsStore.setState({ settings })).catch((e) => {
  // surfaced for debugging only; non-fatal
  // eslint-disable-next-line no-console
  console.error('failed to subscribe to settings://changed', e);
});
