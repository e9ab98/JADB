import { useRef, useState } from 'react';
import { relaunch } from '@tauri-apps/plugin-process';
import { toast } from 'sonner';
import { downloadAndInstall } from '@/lib/update';
import { Button } from '@/components/ui/button';

type Props = { version: string; notes?: string };
type Phase = 'idle' | 'downloading' | 'ready' | 'error';

export function UpdateToast({ version, notes }: Props) {
  const [phase, setPhase] = useState<Phase>('idle');
  const [progress, setProgress] = useState(0);
  const lastUpdate = useRef(0);

  async function updateNow() {
    setPhase('downloading');
    try {
      await downloadAndInstall((value) => {
        const now = Date.now();
        if (value < 100 && now - lastUpdate.current < 100) return;
        lastUpdate.current = now;
        setProgress(value);
      });
      setPhase('ready');
    } catch {
      setPhase('error');
      toast.error('更新下载失败，请稍后重试');
      setPhase('idle');
    }
  }

  return (
    <div className="flex min-w-[320px] flex-col gap-2 p-3">
      <div className="flex items-center justify-between text-sm font-medium">
        <span>发现新版本 v{version}</span>
        <button type="button" onClick={() => toast.dismiss()} className="text-lg leading-none">×</button>
      </div>
      {notes && <div className="max-h-20 overflow-auto whitespace-pre-wrap text-xs text-text-1">{notes}</div>}
      {phase === 'downloading' && (
        <div className="h-1.5 w-full overflow-hidden rounded bg-bg-2">
          <div className="h-full bg-brand transition-[width]" style={{ width: `${progress}%` }} />
        </div>
      )}
      <div className="flex justify-end gap-2">
        {phase === 'ready' ? (
          <Button size="sm" onClick={() => relaunch().catch(() => toast.error('重启失败，请手动重启应用'))}>重启并完成更新</Button>
        ) : (
          <Button size="sm" onClick={updateNow} disabled={phase === 'downloading'}>
            {phase === 'downloading' ? `正在更新 ${progress}%` : phase === 'error' ? '重试更新' : '立即更新'}
          </Button>
        )}
      </div>
    </div>
  );
}
