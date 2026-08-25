import { useRef, useState } from 'react';
import { relaunch } from '@tauri-apps/plugin-process';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { downloadAndInstall, markDismissed } from '@/lib/update';
import { Button } from '@/components/ui/button';

type Props = { version: string; notes?: string | undefined };
type Phase = 'idle' | 'downloading' | 'ready' | 'error';

// Throttle progress bar updates so chunked events don't trigger
// dozens of setState calls per second.
const PROGRESS_THROTTLE_MS = 100;

export function UpdateToast({ version, notes }: Props) {
  const { t } = useTranslation();
  const [phase, setPhase] = useState<Phase>('idle');
  const [progress, setProgress] = useState(0);
  const lastUpdate = useRef(0);

  async function updateNow() {
    setPhase('downloading');
    try {
      await downloadAndInstall((value) => {
        const now = Date.now();
        if (value < 100 && now - lastUpdate.current < PROGRESS_THROTTLE_MS) return;
        lastUpdate.current = now;
        setProgress(value);
      });
      setPhase('ready');
    } catch {
      setPhase('error');
      toast.error(t('update.toast.downloadFailed'));
      setPhase('idle');
    }
  }

  function dismiss() {
    // Persist the dismissed version so the next app launch with the same
    // release doesn't re-surface this toast. The user can still find the
    // update via Settings → Updates.
    markDismissed(version);
    toast.dismiss();
  }

  async function restart() {
    try {
      await relaunch();
    } catch {
      toast.error(t('update.toast.restartFailed'));
    }
  }

  let actionLabel: string;
  if (phase === 'downloading') {
    actionLabel = t('update.toast.progress', { pct: progress });
  } else if (phase === 'error') {
    actionLabel = t('update.toast.retry');
  } else {
    actionLabel = t('update.toast.updateNow');
  }

  return (
    <div className="flex min-w-[320px] flex-col gap-2 p-3">
      <div className="flex items-center justify-between text-sm font-medium">
        <span>{t('update.toast.title', { version })}</span>
        <button type="button" onClick={dismiss} className="text-lg leading-none">×</button>
      </div>
      {notes && <div className="max-h-20 overflow-auto whitespace-pre-wrap text-xs text-text-1">{notes}</div>}
      {phase === 'downloading' && (
        <div className="h-1.5 w-full overflow-hidden rounded bg-bg-2">
          <div className="h-full bg-brand transition-[width]" style={{ width: `${progress}%` }} />
        </div>
      )}
      <div className="flex justify-end gap-2">
        {phase === 'ready' ? (
          <Button size="sm" onClick={restart}>{t('update.toast.restart')}</Button>
        ) : (
          <Button size="sm" onClick={updateNow} disabled={phase === 'downloading'}>
            {actionLabel}
          </Button>
        )}
      </div>
    </div>
  );
}
