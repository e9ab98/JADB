import { useEffect, useState } from 'react';
import { relaunch } from '@tauri-apps/plugin-process';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import {
  checkForUpdate,
  downloadAndInstall,
  getCurrentVersion,
  isNewer,
  shouldNotify,
  setNotify,
  getLastCheckTime,
  LS_KEYS,
  type UpdateInfo,
} from '@/lib/update';

type Phase = 'idle' | 'downloading' | 'ready';

function formatDateTime(iso: string | null, fallback: string): string {
  if (!iso) return fallback;
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return fallback;
  return new Date(t).toLocaleString();
}

export function UpdatesTab() {
  const { t } = useTranslation();

  const [current, setCurrent] = useState<string>('');
  const [latest, setLatest] = useState<UpdateInfo | null>(null);
  const [isChecking, setIsChecking] = useState(false);
  const [hasChecked, setHasChecked] = useState(false);
  const [notify, setNotifyState] = useState<boolean>(shouldNotify());
  const [phase, setPhase] = useState<Phase>('idle');
  const [progress, setProgress] = useState(0);
  // Re-render once per minute so the "last checked X minutes ago"
  // string stays fresh without manual polling of the underlying timestamp.
  const [lastCheckDisplay, setLastCheckDisplay] = useState<string>(
    getLastCheckTime() ?? '',
  );

  useEffect(() => {
    getCurrentVersion()
      .then(setCurrent)
      .catch(() => setCurrent(''));
  }, []);

  useEffect(() => {
    const id = window.setInterval(() => {
      setLastCheckDisplay(getLastCheckTime() ?? '');
    }, 60_000);
    return () => window.clearInterval(id);
  }, []);

  async function onCheck() {
    // "Check now" bypasses the 24h cache so the manual button always
    // hits the network. The Sidebar's button uses `force=true` to do
    // the same; here we just clear the cache and call without force.
    localStorage.removeItem(LS_KEYS.checked);
    setHasChecked(true);
    setIsChecking(true);
    try {
      const info = await checkForUpdate({ timeout: 5000 });
      setLatest(info);
      setLastCheckDisplay(getLastCheckTime() ?? '');
    } catch {
      toast.error(t('settings.updates.checkFailed'));
    } finally {
      setIsChecking(false);
    }
  }

  async function onUpdate() {
    setPhase('downloading');
    setProgress(0);
    try {
      await downloadAndInstall((pct) => setProgress(pct));
      setPhase('ready');
    } catch {
      toast.error(t('settings.updates.checkFailed'));
      setPhase('idle');
    }
  }

  async function onRestart() {
    try {
      await relaunch();
    } catch {
      toast.error(t('settings.updates.restartHint'));
    }
  }

  function onNotifyToggle(next: boolean) {
    setNotifyState(next);
    setNotify(next);
  }

  const updateAvailable =
    !!latest && !!current && isNewer(latest.version, current);
  const neverChecked = hasChecked && latest === null;

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle>{t('settings.updates.title')}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid grid-cols-2 gap-4 text-sm">
            <div>
              <div className="text-text-2">{t('settings.updates.currentVersion')}</div>
              <div className="font-mono text-text-0">{current || t('settings.updates.unknownVersion')}</div>
            </div>
            <div>
              <div className="text-text-2">{t('settings.updates.latestVersion')}</div>
              <div className="font-mono text-text-0">
                {hasChecked
                  ? latest?.version ?? t('settings.updates.upToDate')
                  : '—'}
              </div>
            </div>
            <div className="col-span-2">
              <div className="text-text-2">{t('settings.updates.lastCheck')}</div>
              <div className="text-text-1">
                {formatDateTime(lastCheckDisplay, t('settings.updates.never'))}
              </div>
            </div>
          </div>

          {neverChecked && (
            <div className="text-xs text-text-2">
              {t('settings.updates.upToDate')}
            </div>
          )}

          <div className="flex flex-wrap items-center gap-2">
            <Button
              size="sm"
              variant="outline"
              onClick={onCheck}
              disabled={isChecking}
            >
              {isChecking
                ? t('settings.updates.checking')
                : t('settings.updates.check')}
            </Button>

            {updateAvailable && phase === 'idle' && (
              <Button size="sm" onClick={onUpdate}>
                {t('settings.updates.updateNow')}
              </Button>
            )}

            {phase === 'downloading' && (
              <Button size="sm" disabled>
                {t('settings.updates.downloading', { pct: progress })}
              </Button>
            )}

            {phase === 'ready' && (
              <Button size="sm" onClick={onRestart}>
                {t('settings.updates.restart')}
              </Button>
            )}
          </div>

          {phase === 'downloading' && (
            <div
              role="progressbar"
              aria-valuenow={progress}
              aria-valuemin={0}
              aria-valuemax={100}
              className="h-1.5 w-full overflow-hidden rounded bg-bg-2"
            >
              <div
                className="h-full bg-brand transition-[width] duration-200"
                style={{ width: `${progress}%` }}
              />
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardContent className="pt-6">
          <label className="flex items-center justify-between text-sm">
            <div className="space-y-0.5">
              <div className="text-text-0">{t('settings.updates.notify')}</div>
              <div className="text-xs text-text-2">
                {t('settings.updates.notifyHint')}
              </div>
            </div>
            <Switch
              checked={notify}
              onCheckedChange={onNotifyToggle}
              aria-label={t('settings.updates.notify')}
            />
          </label>
        </CardContent>
      </Card>
    </div>
  );
}
