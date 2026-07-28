import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export const EVT = {
  SettingsChanged: 'settings://changed',
  ToolInstallProgress: 'tool://install-progress',
  TaskProgress: 'task://progress',
  TaskLog: 'task://log',
  TaskDone: 'task://done',
  TaskError: 'task://error',
} as const;

export type TaskLogLevel = 'info' | 'warn' | 'error';

export type ProgressEvent = {
  task_id: string;
  percent: number;
  stage?: string;
};

export type LogEvent = {
  task_id: string;
  line: string;
  level: TaskLogLevel;
};

export type DoneEvent = {
  task_id: string;
  result?: unknown;
};

export type ErrorEvent = {
  task_id: string;
  error: string;
};

export function onTaskProgress(cb: (e: ProgressEvent) => void): Promise<UnlistenFn> {
  return listen<ProgressEvent>(EVT.TaskProgress, (ev) => cb(ev.payload));
}

export function onTaskLog(cb: (e: LogEvent) => void): Promise<UnlistenFn> {
  return listen<LogEvent>(EVT.TaskLog, (ev) => cb(ev.payload));
}

export function onTaskDone(cb: (e: DoneEvent) => void): Promise<UnlistenFn> {
  return listen<DoneEvent>(EVT.TaskDone, (ev) => cb(ev.payload));
}

export function onTaskError(cb: (e: ErrorEvent) => void): Promise<UnlistenFn> {
  return listen<ErrorEvent>(EVT.TaskError, (ev) => cb(ev.payload));
}
