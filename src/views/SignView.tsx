import { useState } from 'react';
import { SignForm } from '@/features/sign/SignForm';
import { TaskPanel } from '@/components/TaskPanel';
import { openPath } from '@/ipc/decompile';
import type { TaskHandle } from '@/ipc/types';

export function SignView() {
  const [task, setTask] = useState<TaskHandle | null>(null);
  return (
    <div className="space-y-6 p-8">
      <SignForm onStarted={setTask} />
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
