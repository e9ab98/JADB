import { useState } from 'react';
import { DecompileForm } from '@/features/decompile/DecompileForm';
import { TaskPanel } from '@/components/TaskPanel';
import type { TaskHandle } from '@/ipc/types';
import { openPath } from '@/ipc/decompile';

export function DecompileView() {
  const [task, setTask] = useState<TaskHandle | null>(null);
  return (
    <div className="space-y-6 p-8">
      <DecompileForm onStarted={setTask} />
      {task && (
        <TaskPanel
          taskId={task.task_id}
          onClose={() => setTask(null)}
          onOpenDir={(p) => openPath(p).catch(() => undefined)}
        />
      )}
    </div>
  );
}
