import { useEffect } from 'react';
import { useSettingsStore } from '@/store/settings';
import type { ThemeMode } from '@/ipc/useTauri';

function applyTheme(mode: ThemeMode): void {
  const root = document.documentElement;
  let resolved: 'light' | 'dark';
  if (mode === 'system') {
    resolved = matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  } else {
    resolved = mode;
  }
  root.dataset.theme = resolved;
  try {
    localStorage.setItem('jadb-theme', mode);
  } catch {
    /* localStorage unavailable */
  }
}

export function useTheme(): void {
  const theme = useSettingsStore((s) => s.settings?.theme ?? null);

  useEffect(() => {
    if (!theme) return;
    applyTheme(theme);
    if (theme !== 'system') return;
    const mq = matchMedia('(prefers-color-scheme: dark)');
    const handler = () => applyTheme('system');
    mq.addEventListener('change', handler);
    return () => mq.removeEventListener('change', handler);
  }, [theme]);
}
