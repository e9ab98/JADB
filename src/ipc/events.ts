import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { Settings } from './useTauri';

export const EVT = {
  SettingsChanged: 'settings://changed',
  ToolInstallProgress: 'tool://install-progress',
  TaskProgress: 'task://progress',
  TaskLog: 'task://log',
  TaskDone: 'task://done',
  TaskError: 'task://error',
} as const;

export function onSettingsChanged(cb: (settings: Settings) => void): Promise<UnlistenFn> {
  return listen<Settings>(EVT.SettingsChanged, (e) => cb(e.payload));
}

