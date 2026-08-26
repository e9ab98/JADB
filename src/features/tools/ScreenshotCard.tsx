import { useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import {
  AlertTriangle,
  Camera,
  Check,
  ChevronDown,
  ChevronUp,
  Image as ImageIcon,
  Loader2,
  Play,
  RotateCcw,
} from 'lucide-react';
import { save as saveDialog } from '@tauri-apps/plugin-dialog';
import { toast } from 'sonner';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { useStepRunner, type Step } from './_shared/stepRunner';
import { safeShell } from './_shared/safeShell';
import { StepRunnerDetails } from './_shared/StepRunnerDetails';
import { pullFile } from '@/ipc/adb';

/**
 * One-shot "capture a screenshot" tool.
 *
 * Pipeline (rendered as 3 steps so the user sees progress instead of
 * a silent 4-second spinner):
 *   1. Run `screencap -p /sdcard/jadb-screenshot.png` on-device.
 *   2. Pop a save dialog so the user picks where to drop the file.
 *   3. Pull the file down via the existing `pull_file` IPC.
 *
 * The image preview is a best-effort `file://` render; the actual
 * PNG only exists on disk after step 3, so the preview only lights
 * up once pull succeeds. We deliberately don't try to load the
 * device-side PNG through the IPC bridge because `read_file` would
 * buffer 5+ MB of PNG bytes per refresh; file:// is cheap and uses
 * the platform image viewer instead.
 *
 * Idempotency: the device-side PNG is overwritten on every run.
 * The local copy is whatever the user picked in the save dialog
 * (default: `jadb-screenshot-<ts>.png`).
 */
export function ScreenshotCard({ serial }: { serial: string }) {
  const { t } = useTranslation();
  const [lastLocal, setLastLocal] = useState<string | null>(null);
  // `previewNonce` flips whenever a fresh screenshot lands on disk so
  // the <img> re-loads even when the user picks the same path twice
  // (browsers otherwise serve the cached file from the prior run).
  const [previewNonce, setPreviewNonce] = useState(0);

  // Bridges the user's save-dialog pick (step 2) into step 3 without
  // expanding the shared `useStepRunner` signature. Single-instance
  // hook, so a useRef is enough -- no globals.
  const localPathRef = useRef<string>('');

  const steps: Step[] = [
    {
      id: 'capture',
      labelKey: 'capture',
      run: async (_s, log) => {
        const out = await safeShell(serial, 'screencap -p /sdcard/jadb-screenshot.png');
        if (!out || out.exitCode !== 0) {
          throw new Error(t('tools.runFailed', {
            error: out?.stderr.trim() || 'screencap failed',
          }));
        }
        log('[capture] ' + t('tools.screenshot.captured'));
        return { ok: true, detail: '/sdcard/jadb-screenshot.png' };
      },
    },
    {
      id: 'pickTarget',
      labelKey: 'pickTarget',
      run: async (_s, log) => {
        const local = await saveDialog({
          title: t('tools.screenshot.saveTitle'),
          defaultPath: `jadb-screenshot-${Date.now()}.png`,
          filters: [{ name: 'PNG', extensions: ['png'] }],
        });
        if (!local) {
          // User cancelled the save dialog. Skip step 3 (no path to
          // pull to) and surface this as a soft failure so the card
          // flips back to "ready" without a scary red badge.
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
          '/sdcard/jadb-screenshot.png',
          localPath,
        );
        log('[pull] ' + result);
        setLastLocal(localPath);
        setPreviewNonce((n) => n + 1);
        return { ok: true, detail: localPath };
      },
    },
  ];

  const runner = useStepRunner(steps, serial);

  async function run() {
    // Reset the bridge so a previous run's path can't leak in if the
    // new run cancels at the dialog.
    localPathRef.current = '';
    const intro = `[start] ${t('tools.screenshot.title')} @ ${serial}`;
    const ok = await runner.run(intro);
    if (ok) {
      toast.success(t('tools.screenshot.success'));
    } else if (localPathRef.current) {
      // Step 3 actually failed -- genuine error toast.
      toast.error(t('tools.runFailed', { error: t('tools.screenshot.title') }));
    }
    // else: user cancelled at the dialog; stay quiet.
  }

  const completedCount = runner.rows.filter((r) => r.status !== 'pending').length;

  return (
    <Card className="overflow-hidden">
      <CardContent className="p-3">
        <div className="flex items-center gap-2.5">
          <div className="grid h-8 w-8 shrink-0 place-items-center rounded-md bg-brand/15 text-brand">
            <Camera className="h-4 w-4" />
          </div>

          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-1.5">
              <h3 className="truncate text-sm font-medium leading-tight text-text-0">
                {t('tools.screenshot.title')}
              </h3>
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
              {t('tools.screenshot.subtitle')}
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
                ? t('tools.screenshot.running')
                : runner.lastResult !== null
                ? t('tools.miuiUsbInstall.retry')
                : t('tools.screenshot.run')}
            </Button>
          </div>
        </div>

        {lastLocal && (
          // Preview block sits between the header and the collapsible
          // details panel so the user gets immediate visual feedback
          // without having to expand the log. `file://` works on every
          // platform Tauri targets today; the nonce (`?v=`) busts the
          // browser's disk cache when the user overwrites the file.
          <div className="mt-3 overflow-hidden rounded-md border border-border bg-bg-2">
            <div className="flex items-center justify-between border-b border-border px-2 py-1 text-[11px] text-text-2">
              <div className="inline-flex items-center gap-1">
                <ImageIcon className="h-3 w-3" />
                {t('tools.screenshot.preview')}
              </div>
              <Badge variant="secondary" className="h-4 px-1 py-0 text-[10px] leading-none">
                {lastLocal.split(/[\\/]/).pop()}
              </Badge>
            </div>
            <div className="grid max-h-56 place-items-center bg-black/40 p-2">
              <img
                src={`file://${lastLocal}?v=${previewNonce}`}
                alt={t('tools.screenshot.preview')}
                className="max-h-52 max-w-full rounded object-contain"
                onError={(e) => {
                  // Hide the broken image silently -- the card still
                  // shows the file path badge so the user can find it.
                  (e.currentTarget as HTMLImageElement).style.display = 'none';
                }}
              />
            </div>
          </div>
        )}

        {runner.detailsOpen && runner.hasDetails && (
          <StepRunnerDetails
            rows={runner.rows}
            logLines={runner.logLines}
            labelNamespace="tools.screenshot.steps"
            outputTitleLabel={t('tools.miuiUsbInstall.outputTitle')}
            copyLogLabel={t('tools.miuiUsbInstall.copyLog')}
            logCopiedLabel={t('tools.miuiUsbInstall.logCopied')}
          />
        )}
      </CardContent>
    </Card>
  );
}
