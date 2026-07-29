import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { Settings } from '@/ipc/useTauri';

/// Build a fully-populated `Settings` for use in mocks. Tests that
/// care about a particular field pass it via `overrides`; everything
/// else defaults to `null` / `undefined`-equivalent. Centralising the
/// shape here means a new field added to `Settings` only needs to be
/// declared once instead of in every mock.
function makeTestSettings(overrides: Partial<Settings> = {}): Settings {
  return {
    aaptPath: null,
    adbPath: null,
    apktoolPath: null,
    uberApkSignerPath: null,
    apksignerPath: null,
    androidBuildToolsDir: null,
    jadxDir: null,
    javaDir: null,
    rulesPath: null,
    rulesDownloadUrl: null,
    language: 'zhcn',
    theme: 'system',
    ...overrides,
  };
}

vi.mock('@/ipc/useTauri', () => ({
  getSettings: vi.fn(async () => makeTestSettings({ language: 'en', theme: 'dark' })),
  updateSettings: vi.fn(async (patch: Record<string, unknown>) =>
    makeTestSettings({
      language: (patch.language as Settings['language']) ?? 'zhcn',
      theme: (patch.theme as Settings['theme']) ?? 'system',
    }),
  ),
}));

vi.mock('@/ipc/events', () => ({
  onSettingsChanged: vi.fn(async () => () => {}),
  EVT: {
    SettingsChanged: 'settings://changed',
    ToolInstallProgress: 'tool://install-progress',
    TaskProgress: 'task://progress',
    TaskLog: 'task://log',
    TaskDone: 'task://done',
    TaskError: 'task://error',
  },
}));

describe('useSettingsStore', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.resetModules();
  });

  it('refresh populates settings', async () => {
    const { useSettingsStore } = await import('@/store/settings');
    await useSettingsStore.getState().refresh();
    expect(useSettingsStore.getState().settings?.language).toBe('en');
    expect(useSettingsStore.getState().settings?.theme).toBe('dark');
  });

  it('setLanguage calls updateSettings and stores result', async () => {
    const { useSettingsStore } = await import('@/store/settings');
    await useSettingsStore.getState().refresh();
    await useSettingsStore.getState().setLanguage('en');
    expect(useSettingsStore.getState().settings?.language).toBe('en');
  });

  it('setTheme calls updateSettings and stores result', async () => {
    const { useSettingsStore } = await import('@/store/settings');
    await useSettingsStore.getState().refresh();
    await useSettingsStore.getState().setTheme('light');
    expect(useSettingsStore.getState().settings?.theme).toBe('light');
  });
});
