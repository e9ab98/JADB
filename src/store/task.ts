import { create } from 'zustand';
import {
  onTaskProgress,
  onTaskLog,
  onTaskDone,
  onTaskError,
  type ProgressEvent,
  type LogEvent,
  type DoneEvent,
  type ErrorEvent,
  type TaskLogLevel,
} from '@/lib/progress';

export type TaskStatus = 'running' | 'done' | 'error' | 'cancelled';

export type TaskState = {
  task_id: string;
  percent: number;
  stage: string;
  status: TaskStatus;
  logs: { line: string; level: TaskLogLevel }[];
  result?: unknown;
  error?: string;
};

type TasksState = {
  tasks: Record<string, TaskState>;
  setTask: (id: string, init: Pick<TaskState, 'percent' | 'stage'>) => void;
  appendLog: (e: LogEvent) => void;
  applyProgress: (e: ProgressEvent) => void;
  applyDone: (e: DoneEvent) => void;
  applyError: (e: ErrorEvent) => void;
  clear: (id: string) => void;
};

export const useTasksStore = create<TasksState>((set) => ({
  tasks: {},
  setTask(id, { percent, stage }) {
    set((s) => ({
      tasks: {
        ...s.tasks,
        [id]: {
          task_id: id,
          percent,
          stage,
          status: 'running',
          logs: s.tasks[id]?.logs ?? [],
          ...(s.tasks[id]?.result !== undefined ? { result: s.tasks[id]!.result } : {}),
          ...(s.tasks[id]?.error !== undefined ? { error: s.tasks[id]!.error } : {}),
        },
      },
    }));
  },
  appendLog({ task_id, line, level }) {
    set((s) => {
      const cur = s.tasks[task_id];
      if (!cur) return s;
      return { tasks: { ...s.tasks, [task_id]: { ...cur, logs: [...cur.logs, { line, level }] } } };
    });
  },
  applyProgress({ task_id, percent, stage }) {
    set((s) => {
      const cur = s.tasks[task_id];
      if (!cur) {
        return {
          tasks: {
            ...s.tasks,
            [task_id]: { task_id, percent, stage: stage ?? '', status: 'running', logs: [] },
          },
        };
      }
      return {
        tasks: {
          ...s.tasks,
          [task_id]: { ...cur, percent, stage: stage ?? cur.stage },
        },
      };
    });
  },
  applyDone({ task_id, result }) {
    set((s) => {
      const cur = s.tasks[task_id];
      if (!cur) return s;
      return {
        tasks: {
          ...s.tasks,
          [task_id]: { ...cur, percent: 100, status: 'done', ...(result !== undefined ? { result } : {}) },
        },
      };
    });
  },
  applyError({ task_id, error }) {
    set((s) => {
      const cur = s.tasks[task_id];
      const status: TaskStatus = error.includes('cancelled') || error.includes('cancel') || error.includes('取消') ? 'cancelled' : 'error';
      if (!cur) {
        return {
          tasks: {
            ...s.tasks,
            [task_id]: { task_id, percent: 0, stage: '', status, logs: [], error },
          },
        };
      }
      return {
        tasks: {
          ...s.tasks,
          [task_id]: { ...cur, status, error },
        },
      };
    });
  },
  clear(id) {
    set((s) => {
      const next = { ...s.tasks };
      delete next[id];
      return { tasks: next };
    });
  },
}));

// Subscribe to backend task events once at module load.
onTaskProgress((e) => useTasksStore.getState().applyProgress(e)).catch(() => undefined);
onTaskLog((e) => useTasksStore.getState().appendLog(e)).catch(() => undefined);
onTaskDone((e) => useTasksStore.getState().applyDone(e)).catch(() => undefined);
onTaskError((e) => useTasksStore.getState().applyError(e)).catch(() => undefined);
