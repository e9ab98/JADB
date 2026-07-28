import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import { ThemeProvider } from '@/components/ThemeProvider';
import { useSettingsStore } from '@/store/settings';
import { localeFor } from '@/i18n';
import './i18n';
import './styles/globals.css';

// Kick off settings load + sync i18n once before first render.
async function bootstrap() {
  try {
    await useSettingsStore.getState().refresh();
    const lang = useSettingsStore.getState().settings?.language ?? 'zhcn';
    await import('./i18n').then((m) => m.default.changeLanguage(localeFor(lang)));
  } catch (e) {
    // Settings unavailable (e.g. outside Tauri) — keep defaults.
    // eslint-disable-next-line no-console
    console.warn('settings unavailable; using defaults', e);
  }

  ReactDOM.createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
      <ThemeProvider>
        <App />
      </ThemeProvider>
    </React.StrictMode>,
  );
}

void bootstrap();
