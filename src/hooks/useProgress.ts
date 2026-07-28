import { useTasksStore } from '@/store/task';

/**
 * Subscribe to a single task's state. Returns the TaskState (or null if no events
 * for this task_id have been observed yet — i.e. the task hasn't started or is
 * unknown).
 */
export function useProgress(taskId: string | null) {
  return useTasksStore((s) => (taskId ? s.tasks[taskId] ?? null : null));
}
