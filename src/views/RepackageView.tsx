import { useState } from 'react';
import { RepackageForm } from '@/features/repackage/RepackageForm';
import { TaskPanel } from '@/components/TaskPanel';
import { openPath } from '@/ipc/decompile';
import type { TaskHandle } from '@/ipc/types';

export function RepackageView() {
  const [task, setTask] = useState<TaskHandle | null>(null);
  return (
    <div className="space-y-6 p-8">
      <RepackageForm onStarted={setTask} />
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
