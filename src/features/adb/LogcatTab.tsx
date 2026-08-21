// Minimal logcat tab: capture-to-file on device, then pull to local and
// open in an external editor. Live streaming is intentionally NOT here —
// pushing every line through Tauri IPC and re-rendering a virtualized
// list on every line costs both latency and battery, and most debugging
// flows only need the captured file after the fact.
//
// Package list comes from the shared `usePackagesStore`, which fetches
// once per device and is shared with the Apps tab. No per-tab fetch.
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import { save } from '@tauri-apps/plugin-dialog';
import { toast } from 'sonner';
import { Download, Loader2, Smartphone } from 'lucide-react';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { adbLogcatCapture, adbLogcatPull } from '@/ipc/adb';
import { Combobox } from '@/components/ui/combobox';
import { usePackagesStore, selectInfos } from '@/store/packages';

type Props = {
  serial: string;
};

type CaptureResult = {
  remotePath: string;
  lineCount: number;
};

/** Builds `/data/local/tmp/jadb-logcat-<ts>.log`. Shell uid can write
 *  here on every Android version, so the capture doesn't need any
 *  storage permission on the device side. */
function makeRemotePath(): string {
  return `/data/local/tmp/jadb-logcat-${Date.now()}.log`;
}

export function LogcatTab({ serial }: Props) {
  const { t } = useTranslation();
  const [packageName, setPackageName] = useState('');
  const [durationSecs, setDurationSecs] = useState<number>(0);
  const [capturing, setCapturing] = useState(false);
  const [result, setResult] = useState<CaptureResult | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Subscribe to the shared package cache for this serial.
  const packages = usePackagesStore((s) => selectInfos(s, serial).map((i) => i.packageName));
  const packagesLoading = usePackagesStore((s) => s.loading[serial] ?? false);
  const packagesError = usePackagesStore((s) => s.error[serial] ?? null);
  const ensureLoaded = usePackagesStore((s) => s.ensureLoaded);

  // Kick the shared fetch on mount (or when serial changes). The store
  // is idempotent so multiple tabs calling ensureLoaded for the same
  // serial coalesce into a single in-flight fetch.
  useEffect(() => {
    void ensureLoaded(serial);
  }, [serial, ensureLoaded]);

  async function capture() {
    setCapturing(true);
    setError(null);
    setResult(null);
    const remote = makeRemotePath();
    try {
      const lineCount = await adbLogcatCapture(
        serial,
        packageName.trim() || null,
        remote,
        durationSecs,
      );
      setResult({ remotePath: remote, lineCount });
      toast.success(
        t('adb.adbLogcat.captureDone', {
          count: lineCount,
          path: remote,
        }),
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setCapturing(false);
    }
  }

  async function download() {
    if (!result) return;
    setDownloading(true);
    setError(null);
    try {
      const localPath = await save({
        title: t('adb.adbLogcat.saveTitle'),
        defaultPath: `logcat-${Date.now()}.log`,
        filters: [{ name: 'Log', extensions: ['log', 'txt'] }],
      });
      if (!localPath) {
        // User cancelled the save dialog — that's not an error.
        return;
      }
      const final = await adbLogcatPull(serial, result.remotePath, localPath);
      toast.success(t('adb.adbLogcat.downloadDone', { path: final }));
    } catch (e) {
      setError(String(e));
    } finally {
      setDownloading(false);
    }
  }

  // Combobox shows the shared package list. While loading it stays
  // interactive (the user can still pick from the cached list, or
  // type if nothing's loaded yet). When the fetch errored AND there's
  // no cached list to show, fall back to a plain Input.
  const noCachedPackages = packages.length === 0;
  const showInputFallback = !!packagesError && noCachedPackages;

  return (
    <div className="space-y-4">
      <Card>
        <CardContent className="space-y-4">
          <div className="flex items-center gap-2 text-sm font-semibold text-text-0">
            <Smartphone className="h-4 w-4 text-text-1" />
            {t('adb.adbLogcat.captureTitle')}
          </div>
          <div className="flex flex-wrap items-center gap-2">
            {showInputFallback ? (
              <Input
                value={packageName}
                onChange={(e) => setPackageName(e.target.value)}
                placeholder={t('adb.adbLogcat.packagePlaceholder')}
                disabled={capturing}
                className="min-w-[180px] flex-1 font-mono text-sm"
              />
            ) : (
              <Combobox
                value={packageName}
                onChange={setPackageName}
                options={packages}
                placeholder={t('adb.adbLogcat.packagePlaceholder')}
                filterPlaceholder={t('adb.adbLogcat.comboboxFilter')}
                emptyText={t('adb.adbLogcat.comboboxEmpty')}
                emptyLabel={t('adb.adbLogcat.allPackages')}
                allowEmpty
                // Disable ONLY during active capture, NOT during fetch
                // (the cached list is interactive immediately).
                disabled={capturing}
                loading={packagesLoading}
                className="min-w-[180px] flex-1"
              />
            )}
            <label className="flex items-center gap-1.5 text-xs text-text-2">
              <span>{t('adb.adbLogcat.durationLabel')}</span>
              <Input
                type="number"
                min={0}
                max={600}
                step={1}
                value={durationSecs}
                onChange={(e) =>
                  setDurationSecs(Math.max(0, Number(e.target.value) || 0))
                }
                disabled={capturing}
                className="w-20 font-mono text-sm"
                title={t('adb.adbLogcat.durationTooltip')}
              />
              <span>s</span>
            </label>
            <Button onClick={capture} disabled={capturing}>
              {capturing ? (
                <>
                  <Loader2 className="h-4 w-4 animate-spin" />
                  {durationSecs > 0
                    ? t('adb.adbLogcat.capturingDuration', { s: durationSecs })
                    : t('adb.adbLogcat.capturing')}
                </>
              ) : (
                t('adb.adbLogcat.capture')
              )}
            </Button>
          </div>
          <p className="text-xs text-text-2">
            {packagesError && noCachedPackages
              ? t('adb.adbLogcat.packagesLoadFailed', { error: packagesError })
              : t('adb.adbLogcat.captureHint')}
          </p>
        </CardContent>
      </Card>

      {result && (
        <Card>
          <CardContent className="space-y-3">
            <div className="flex flex-wrap items-center gap-3 text-sm">
              <Badge variant="secondary">
                {t('adb.adbLogcat.lineCount', { count: result.lineCount })}
              </Badge>
              <code className="break-all font-mono text-xs text-text-1">
                {result.remotePath}
              </code>
            </div>
            <Button onClick={download} disabled={downloading}>
              {downloading ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Download className="h-4 w-4" />
              )}
              {t('adb.adbLogcat.download')}
            </Button>
          </CardContent>
        </Card>
      )}

      {error && (
        <Card className="border-danger">
          <CardContent className="text-sm text-danger">{error}</CardContent>
        </Card>
      )}
    </div>
  );
}
