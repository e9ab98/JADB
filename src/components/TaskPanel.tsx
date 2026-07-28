import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import { Progress } from '@/components/ui/progress';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { useProgress } from '@/hooks/useProgress';
import { invoke } from '@tauri-apps/api/core';
import { Pause, Play, X, FolderOpen } from 'lucide-react';
import { cn } from '@/lib/utils';

type Props = {
  taskId: string | null;
  onClose?: () => void;
  onOpenDir?: (path: string) => void;
};

export function TaskPanel({ taskId, onClose, onOpenDir }: Props) {
  const { t } = useTranslation();
  const task = useProgress(taskId);
  const [paused, setPaused] = useState(false);
  const visibleLogs = paused ? (task?.logs ?? []).slice(-200) : task?.logs ?? [];

  if (!task) {
    return (
      <div className="rounded-2xl border border-dashed border-border bg-bg-1 p-6 text-sm text-text-2">
        {t('common.cancel')}
      </div>
    );
  }

  const percent = Math.max(0, Math.min(100, task.percent));
  const stageLabel = task.stage || (task.status === 'done' ? '100%' : '…');

  return (
    <div className="rounded-2xl border border-border bg-bg-1 shadow-card">
      <div className="flex items-center gap-3 border-b border-border p-4">
        <div className="flex-1">
          <Progress value={percent} />
        </div>
        <div className="w-32 text-right text-xs text-text-2">{stageLabel}</div>
        <Button size="sm" variant="outline" onClick={() => setPaused((p) => !p)}>
          {paused ? <Play className="h-4 w-4" /> : <Pause className="h-4 w-4" />}
        </Button>
        {task.status === 'running' && taskId && (
          <Button
            size="sm"
            variant="outline"
            onClick={() => invoke('cancel_task', { taskId }).catch(() => undefined)}
          >
            <X className="h-4 w-4" />
          </Button>
        )}
        {task.status === 'done' && typeof task.result === 'string' && onOpenDir && (
          <Button size="sm" variant="outline" onClick={() => onOpenDir(task.result as string)}>
            <FolderOpen className="h-4 w-4" />
          </Button>
        )}
        {onClose && (
          <Button size="sm" variant="ghost" onClick={onClose}>
            {t('common.cancel')}
          </Button>
        )}
      </div>
      <ScrollArea className={cn('h-72 p-4')}>
        <pre className="whitespace-pre-wrap break-all font-mono text-xs leading-relaxed">
          {visibleLogs.map((l, i) => (
            <div key={i} className={cn(l.level === 'error' && 'text-danger', l.level === 'warn' && 'text-warning')}>
              {l.line}
            </div>
          ))}
        </pre>
      </ScrollArea>
      {task.status === 'error' && (
        <div className="border-t border-danger/30 bg-danger/5 p-3 text-sm text-danger">{task.error}</div>
      )}
      {task.status === 'cancelled' && (
        <div className="border-t border-warning/30 bg-warning/5 p-3 text-sm text-warning">
          {t('common.cancel')}led
        </div>
      )}
    </div>
  );
}
