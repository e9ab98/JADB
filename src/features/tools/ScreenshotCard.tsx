import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import { convertFileSrc } from '@tauri-apps/api/core';
import {
  AlertTriangle,
  Camera,
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
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { useStepRunner, type Step } from './_shared/stepRunner';
import { safeShell } from './_shared/safeShell';
import { StepRunnerDetails } from './_shared/StepRunnerDetails';
import {
  screenshotDiscardCache,
  screenshotPullToCache,
  screenshotSaveFromCache,
} from '@/ipc/adb';

/**
 * One-shot "capture a screenshot" tool.
 *
 * Pipeline (rendered as 2 steps so the user sees progress instead of
 * a silent 4-second spinner):
 *   1. Run `screencap -p /sdcard/jadb-screenshot.png` on-device.
 *   2. Pull the file down to the application cache.
 *
 * The local copy is removed when the user either saves it to a final
 * destination or discards the preview. The preview therefore does not
 * need to pass the PNG bytes through the IPC bridge or the platform dialog.
 *
 * Idempotency: the device-side PNG is overwritten on every run.
 * The application cache copy is replaced on every successful pull.
 */
export function ScreenshotCard({ serial }: { serial: string }) {
  const { t } = useTranslation();
  const [previewOpen, setPreviewOpen] = useState(false);
  const [previewPath, setPreviewPath] = useState<string>('');
  // `previewNonce` busts the browser's cache when a fresh screenshot is
  // pulled to the same application cache path.
  const [previewNonce, setPreviewNonce] = useState(0);
  const [previewBusy, setPreviewBusy] = useState(false);

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
      id: 'pullToCache',
      labelKey: 'pullToCache',
      run: async (_s, log) => {
        const localPath = await screenshotPullToCache(serial);
        log('[pullToCache] ' + localPath);
        setPreviewPath(localPath);
        setPreviewNonce((n) => n + 1);
        return { ok: true, detail: localPath };
      },
    },
  ];

  const runner = useStepRunner(steps, serial);

  async function run() {
    const intro = `[start] ${t('tools.screenshot.title')} @ ${serial}`;
    const ok = await runner.run(intro);
    if (ok) {
      setPreviewOpen(true);
    } else {
      toast.error(t('tools.runFailed', { error: t('tools.screenshot.title') }));
    }
  }

  async function savePreview() {
    if (!previewPath) return;
    setPreviewBusy(true);
    try {
      const localPath = await saveDialog({
        title: t('tools.screenshot.saveTitle'),
        defaultPath: `jadb-screenshot-${Date.now()}.png`,
        filters: [{ name: 'PNG', extensions: ['png'] }],
      });
      if (!localPath) return;

      const savedPath = await screenshotSaveFromCache(localPath);
      setPreviewOpen(false);
      setPreviewPath('');
      toast.success(t('tools.screenshot.success', { path: savedPath }));
    } catch (error) {
      toast.error(t('tools.runFailed', { error: String(error) }));
    } finally {
      setPreviewBusy(false);
    }
  }

  async function discardPreview() {
    if (!previewPath) return;
    setPreviewBusy(true);
    try {
      await screenshotDiscardCache();
      setPreviewOpen(false);
      setPreviewPath('');
    } catch (error) {
      toast.error(t('tools.runFailed', { error: String(error) }));
    } finally {
      setPreviewBusy(false);
    }
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

        <Dialog open={previewOpen} onOpenChange={setPreviewOpen}>
          <DialogContent className="max-w-3xl">
            <DialogHeader>
              <DialogTitle>{t('tools.screenshot.previewTitle')}</DialogTitle>
              <DialogDescription>
                {t('tools.screenshot.previewDescription')}
              </DialogDescription>
            </DialogHeader>
            {previewPath && (
              <div className="grid max-h-[60vh] place-items-center overflow-hidden rounded-lg bg-black/40 p-2">
                <img
                  src={`${convertFileSrc(previewPath)}?v=${previewNonce}`}
                  alt={t('tools.screenshot.preview')}
                  className="max-h-[55vh] max-w-full rounded object-contain"
                  onError={(e) => {
                    (e.currentTarget as HTMLImageElement).style.display = 'none';
                  }}
                />
              </div>
            )}
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => void discardPreview()}
                disabled={previewBusy}
              >
                {t('tools.screenshot.discard')}
              </Button>
              <Button
                type="button"
                onClick={() => void savePreview()}
                disabled={previewBusy || !previewPath}
              >
                {previewBusy && <Loader2 className="h-4 w-4 animate-spin" />}
                {t('tools.screenshot.save')}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </CardContent>
    </Card>
  );
}
