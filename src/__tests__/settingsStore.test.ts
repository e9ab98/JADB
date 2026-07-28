import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('@/ipc/useTauri', () => ({
  getSettings: vi.fn(async () => ({
    aaptPath: null,
    apktoolPath: null,
    uberApkSignerPath: null,
    apksignerPath: null,
    jadxDir: null,
    javaDir: null,
    rulesPath: null,
    language: 'en',
    theme: 'dark',
  })),
  updateSettings: vi.fn(async (patch: Record<string, unknown>) => ({
    aaptPath: null,
    apktoolPath: null,
    uberApkSignerPath: null,
    apksignerPath: null,
    jadxDir: null,
    javaDir: null,
    rulesPath: null,
    language: (patch.language as string) ?? 'zhcn',
    theme: (patch.theme as string) ?? 'system',
  })),
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
