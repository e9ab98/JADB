import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import {
  AlertTriangle,
  Bug,
  Check,
  ChevronDown,
  ChevronUp,
  Loader2,
  Play,
  RotateCcw,
} from 'lucide-react';
import { save as saveDialog } from '@tauri-apps/plugin-dialog';
import { toast } from 'sonner';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { useStepRunner, type Step } from './_shared/stepRunner';
import { safeShell } from './_shared/safeShell';
import { StepRunnerDetails } from './_shared/StepRunnerDetails';
import { pullFile } from '@/ipc/adb';

/**
 * One-click BugReport capture + download.
 *
 * Pipeline:
 *   1. `adb shell bugreport <remotePath>` -- this is the slow step
 *      (5-15 minutes on a stock Pixel; sometimes 30+ on a Xiaomi
 *      build with verbose dropbox queues). The Rust side blocks on
 *      `bugreport` for the entire duration; we surface that to the
 *      user with a moving progress bar so the card doesn't look
 *      frozen.
 *   2. Save dialog (user picks the local destination -- bugreport
 *      files are large (50-200 MB typical, >500 MB on verbose OEM
 *      builds) so we don't auto-write anywhere).
 *   3. `adb pull` to copy the .zip down.
 *
 * Why not stream progress: `bugreport` writes the entire .zip to
 * the device's `/sdcard` in one shot, so there's no per-byte
 * progress to surface. The progress bar is a deliberately vague
 * indeterminable timer; a real progress % would need a custom Rust
 * task registry (see `TaskRegistry` in the project -- out of scope
 * here).
 *
 * Why two separate steps: the bugreport phase can fail (SELinux
 * denying the dump, out-of-space, etc.). Splitting it from the
 * pull means a save dialog cancellation in step 2 doesn't pretend
 * step 1 failed.
 */
export function BugReportCard({ serial }: { serial: string }) {
  const { t } = useTranslation();
  const [lastLocal, setLastLocal] = useState<string | null>(null);
  const [elapsed, setElapsed] = useState(0);
  // Bridges step 1's path into step 2 (the save dialog picks the
  // local target, not the remote -- but we still need the chosen
  // local target to feed into the pull). Single-instance hook, so
  // a useRef is enough.
  const localPathRef = useRef<string>('');

  // Indeterminate "we're doing something" indicator. We tick every
  // second while a step is running and render a progress bar that
  // saturates after 5 minutes -- by then the user knows it's a long
  // wait either way, and we don't pretend to know the actual %.
  const [runningSeconds, setRunningSeconds] = useState(0);
  const stepRunning = useRef(false);
  useEffect(() => {
    if (!stepRunning.current) return;
    setRunningSeconds(0);
    const start = Date.now();
    const id = setInterval(() => {
      setRunningSeconds(Math.floor((Date.now() - start) / 1000));
      setElapsed((e) => e + 1);
    }, 1000);
    return () => {
      clearInterval(id);
    };
  }, [stepRunning.current]);

  const steps: Step[] = [
    {
      id: 'capture',
      labelKey: 'capture',
      run: async (_s, log) => {
        stepRunning.current = true;
        const startedAt = Date.now();
        try {
          // `bugreport` (without args) dumps to /sdcard/bugreports/
          // with a timestamped filename -- pinning to a fixed path
          // makes the pull step deterministic and lets the user
          // re-run the tool without manual cleanup.
          const remote = '/sdcard/jadb-bugreport.zip';
          const out = await safeShell(serial, `bugreport ${remote}`);
          const dur = Math.floor((Date.now() - startedAt) / 1000);
          if (!out || out.exitCode !== 0) {
            throw new Error(t('tools.runFailed', {
              error: out?.stderr.trim() || out?.stdout.trim() || 'bugreport failed',
            }));
          }
          // bugreport emits human-readable progress on stderr
          // ("* Stopping framework..." etc.) -- forward the last
          // few lines so the log shows what actually happened.
          const tail = (out.stderr || out.stdout || '').trim().split(/\r?\n/).slice(-6);
          for (const line of tail) {
            if (line.trim()) log('[bugreport] ' + line.trim());
          }
          log(`[bugreport] finished in ${dur}s`);
          return { ok: true, detail: `${remote} (${dur}s)` };
        } finally {
          stepRunning.current = false;
        }
      },
    },
    {
      id: 'pickTarget',
      labelKey: 'pickTarget',
      run: async (_s, log) => {
        const local = await saveDialog({
          title: t('tools.bugReport.saveTitle'),
          defaultPath: `jadb-bugreport-${Date.now()}.zip`,
          filters: [{ name: 'ZIP', extensions: ['zip'] }],
        });
        if (!local) {
          return { ok: false, detail: t('tools.screenshot.saveCancelled') };
        }
        localPathRef.current = local;
        log('[pickTarget] ' + local);
        return { ok: true, detail: local };
      },
    },
    {
      id: 'pull',
      labelKey: 'pull',
      run: async (_s, log) => {
        const localPath = localPathRef.current;
        if (!localPath) {
          throw new Error(t('tools.screenshot.missingLocalPath'));
        }
        const result = await pullFile(
          serial,
          '/sdcard/jadb-bugreport.zip',
          localPath,
        );
        log('[pull] ' + result);
        setLastLocal(localPath);
        return { ok: true, detail: localPath };
      },
    },
  ];

  const runner = useStepRunner(steps, serial);

  // Track the elapsed seconds during the entire run (not just step 1)
  // for the progress bar saturation. Reset on every run.
  useEffect(() => {
    if (runner.running) {
      setElapsed(0);
    }
  }, [runner.running]);

  async function run() {
    localPathRef.current = '';
    setLastLocal(null);
    const intro = `[start] ${t('tools.bugReport.title')} @ ${serial}`;
    const ok = await runner.run(intro);
    if (ok) {
      toast.success(t('tools.bugReport.success'));
    } else if (localPathRef.current) {
      toast.error(t('tools.runFailed', { error: t('tools.bugReport.title') }));
    }
  }

  const completedCount = runner.rows.filter((r) => r.status !== 'pending').length;
  // Saturate the bar at 5 minutes (= 300 seconds). After that we
  // just show "still running" so the user knows the tool isn't hung.
  const progressPct = Math.min(100, Math.round((elapsed / 300) * 100));

  return (
    <Card className="overflow-hidden">
      <CardContent className="p-3">
        <div className="flex items-center gap-2.5">
          <div className="grid h-8 w-8 shrink-0 place-items-center rounded-md bg-warning/15 text-warning">
            <Bug className="h-4 w-4" />
          </div>
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-1.5">
              <h3 className="truncate text-sm font-medium leading-tight text-text-0">
                {t('tools.bugReport.title')}
              </h3>
              <Badge variant="warning" className="h-4 px-1 py-0 text-[10px] leading-none">
                slow
              </Badge>
              {runner.lastResult === 'success' && (
                <span
                  className="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full bg-success/15 text-success"
                  aria-label={t('tools.miuiUsbInstall.resultSuccess')}
                  title={t('tools.miuiUsbInstall.resultSuccess')}
                >
                  <Check className="h-3 w-3" />
                </span>
              )}
              {runner.lastResult === 'failed' && (
                <span
                  className="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full bg-danger/15 text-danger"
                  aria-label={t('tools.miuiUsbInstall.resultFailed')}
                  title={t('tools.miuiUsbInstall.resultFailed')}
                >
                  <AlertTriangle className="h-3 w-3" />
                </span>
              )}
              {runner.running && (
                <span className="inline-flex shrink-0 items-center gap-0.5 text-[10px] text-text-2">
                  <Loader2 className="h-3 w-3 animate-spin" />
                  {completedCount}/{runner.rows.length}
                </span>
              )}
            </div>
            <p className="truncate text-[11px] leading-tight text-text-2">
              {t('tools.bugReport.subtitle')}
            </p>
          </div>

          <div className="flex shrink-0 items-center gap-1">
            {runner.hasDetails && (
              <button
                type="button"
                onClick={() => runner.setDetailsOpen((v) => !v)}
                aria-expanded={runner.detailsOpen}
                aria-label={
                  runner.detailsOpen
                    ? t('tools.miuiUsbInstall.hideDetails')
                    : t('tools.miuiUsbInstall.showDetails')
                }
                className="grid h-7 w-7 place-items-center rounded-md text-text-2 transition-colors hover:bg-bg-2 hover:text-text-0"
              >
                {runner.detailsOpen ? (
                  <ChevronUp className="h-3.5 w-3.5" />
                ) : (
                  <ChevronDown className="h-3.5 w-3.5" />
                )}
              </button>
            )}
            <Button
              onClick={() => void run()}
              disabled={runner.running}
              size="sm"
              className="h-7 gap-1 px-2.5"
            >
              {runner.running ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : runner.lastResult !== null ? (
                <RotateCcw className="h-3.5 w-3.5" />
              ) : (
                <Play className="h-3.5 w-3.5" />
              )}
              {runner.running
                ? t('tools.bugReport.running')
                : runner.lastResult !== null
                ? t('tools.miuiUsbInstall.retry')
                : t('tools.bugReport.run')}
            </Button>
          </div>
        </div>

        {runner.running && (
          // Indeterminate-ish progress: counts seconds, saturates at
          // 5 minutes. The user gets visual feedback without us
          // having to invent a fake percent.
          <div className="mt-3 space-y-1 rounded-md border border-border bg-bg-2/40 p-2">
            <div className="flex items-center justify-between text-[11px] text-text-2">
              <span>
                {t('tools.bugReport.elapsed', { seconds: runningSeconds })}
              </span>
              {elapsed >= 300 && (
                <span className="text-warning">
                  {t('tools.bugReport.stillRunning')}
                </span>
              )}
            </div>
            <Progress value={progressPct} />
          </div>
        )}

        {lastLocal && (
          <div className="mt-3 rounded-md border border-border bg-bg-2/40 p-2 text-[11px] text-text-1">
            <span className="text-text-2">{t('tools.bugReport.savedAt')}</span>{' '}
            <span className="font-mono break-all">{lastLocal}</span>
          </div>
        )}

        {runner.detailsOpen && runner.hasDetails && (
          <StepRunnerDetails
            rows={runner.rows}
            logLines={runner.logLines}
            labelNamespace="tools.bugReport.steps"
            outputTitleLabel={t('tools.miuiUsbInstall.outputTitle')}
            copyLogLabel={t('tools.miuiUsbInstall.copyLog')}
            logCopiedLabel={t('tools.miuiUsbInstall.logCopied')}
          />
        )}
      </CardContent>
    </Card>
  );
}
