import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Loader2, Trash2 } from 'lucide-react';
import { useSettingsStore } from '@/store/settings';
import { localeFor } from '@/i18n';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import type { ThemeMode } from '@/ipc/useTauri';
import {
  clearCache,
  scanCache,
  type CacheClearResult,
  type CacheScanResult,
} from '@/ipc/cache';

const THEMES: ThemeMode[] = ['system', 'light', 'dark'];

type ScanState =
  | { kind: 'loading' }
  | { kind: 'ok'; data: CacheScanResult }
  | { kind: 'error'; message: string };

/** Convert raw bytes to a human-readable string with units KB/MB/GB/TB. */
function humanBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ['KB', 'MB', 'GB', 'TB'];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  return `${v.toFixed(2)} ${units[i]}`;
}

export function GeneralTab() {
  const { t, i18n } = useTranslation();
  const settings = useSettingsStore((s) => s.settings);

  const [scan, setScan] = useState<ScanState>({ kind: 'loading' });
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [clearing, setClearing] = useState(false);
  const [clearingError, setClearingError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void scanCache()
      .then((data) => {
        if (cancelled) return;
        setScan({ kind: 'ok', data });
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        setScan({
          kind: 'error',
          message: e instanceof Error ? e.message : String(e),
        });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const refreshScan = () => {
    setScan({ kind: 'loading' });
    void scanCache()
      .then((data) => setScan({ kind: 'ok', data }))
      .catch((e: unknown) =>
        setScan({
          kind: 'error',
          message: e instanceof Error ? e.message : String(e),
        }),
      );
  };

  const runClear = async () => {
    setClearing(true);
    setClearingError(null);
    try {
      const result = await clearCache();
      handleResult(result);
      setConfirmOpen(false);
    } catch (e: unknown) {
      setClearingError(e instanceof Error ? e.message : String(e));
      toast.error(
        t('settings.cacheClearError', {
          message: e instanceof Error ? e.message : String(e),
        }),
      );
    } finally {
      setClearing(false);
      refreshScan();
    }
  };

  const handleResult = (result: CacheClearResult) => {
    if (result.errors.length === 0) {
      toast.success(
        t('settings.cacheCleared', {
          count: result.deletedFiles,
          size: humanBytes(result.deletedBytes),
        }),
      );
    } else {
      toast.warning(
        t('settings.cacheClearedPartial', {
          ok: result.deletedFiles,
          err: result.errors.length,
        }),
      );
    }
  };

  const clearDisabled = useMemo(
    () => scan.kind !== 'ok' || clearing,
    [scan.kind, clearing],
  );

  if (!settings) return null;

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle>{t('settings.language')}</CardTitle>
        </CardHeader>
        <CardContent className="flex gap-2">
          <Button
            variant={settings.language === 'zhcn' ? 'default' : 'outline'}
            onClick={() =>
              useSettingsStore
                .getState()
                .setLanguage('zhcn')
                .then(() => i18n.changeLanguage(localeFor('zhcn')))
            }
          >
            {t('settings.langZhCn')}
          </Button>
          <Button
            variant={settings.language === 'en' ? 'default' : 'outline'}
            onClick={() =>
              useSettingsStore
                .getState()
                .setLanguage('en')
                .then(() => i18n.changeLanguage(localeFor('en')))
            }
          >
            {t('settings.langEn')}
          </Button>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t('settings.theme')}</CardTitle>
        </CardHeader>
        <CardContent className="flex gap-2">
          {THEMES.map((m) => (
            <Button
              key={m}
              variant={settings.theme === m ? 'default' : 'outline'}
              onClick={() => useSettingsStore.getState().setTheme(m)}
            >
              {m === 'system'
                ? t('settings.themeSystem')
                : m === 'light'
                  ? t('settings.themeLight')
                  : t('settings.themeDark')}
            </Button>
          ))}
        </CardContent>
      </Card>

      <CacheCard
        scan={scan}
        clearDisabled={clearDisabled}
        onClearClick={() => setConfirmOpen(true)}
      />

      <ConfirmDialog
        open={confirmOpen}
        scan={scan}
        clearing={clearing}
        clearingError={clearingError}
        onOpenChange={(o) => !clearing && setConfirmOpen(o)}
        onCancel={() => !clearing && setConfirmOpen(false)}
        onConfirm={runClear}
      />
    </div>
  );
}

function CacheCard({
  scan,
  clearDisabled,
  onClearClick,
}: {
  scan: ScanState;
  clearDisabled: boolean;
  onClearClick: () => void;
}) {
  const { t } = useTranslation();
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('settings.cacheTitle')}</CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        {scan.kind === 'loading' && (
          <div className="flex items-center gap-2 text-sm text-text-2">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t('settings.cacheLoading')}
          </div>
        )}
        {scan.kind === 'error' && (
          <div className="text-sm text-danger">
            {t('settings.cacheError', { message: scan.message })}
          </div>
        )}
        {scan.kind === 'ok' && (
          <div className="space-y-1.5">
            <div className="flex items-baseline justify-between">
              <span className="text-sm font-semibold text-text-0">
                {t('settings.cacheTotal')}
              </span>
              <span className="font-mono text-sm font-semibold text-text-0">
                {humanBytes(scan.data.totalBytes)}
              </span>
            </div>
            <div className="space-y-0.5 text-xs">
              {scan.data.categories.map((cat) => (
                <div
                  key={cat.id}
                  className="flex items-center justify-between"
                >
                  <span className="text-text-2">{cat.label}</span>
                  <span className="font-mono text-text-2">
                    {humanBytes(cat.bytes)} ·{' '}
                    {t('settings.cacheFilesSuffix', { count: cat.fileCount })}
                  </span>
                </div>
              ))}
            </div>
          </div>
        )}
        <Button
          variant="danger"
          disabled={clearDisabled}
          onClick={onClearClick}
        >
          <Trash2 className="h-4 w-4" />
          {t('settings.clearCache')}
        </Button>
      </CardContent>
    </Card>
  );
}

function ConfirmDialog({
  open,
  scan,
  clearing,
  clearingError,
  onOpenChange,
  onCancel,
  onConfirm,
}: {
  open: boolean;
  scan: ScanState;
  clearing: boolean;
  clearingError: string | null;
  onOpenChange: (open: boolean) => void;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const { t } = useTranslation();
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {/* Cap the dialog at 80vh and turn the body into the only scrolling
          region. Long cache previews can otherwise grow the dialog past the
          viewport edge and clip the action buttons. */}
      <DialogContent className="flex max-h-[80vh] flex-col">
        <DialogHeader className="flex-shrink-0">
          <DialogTitle>{t('settings.clearCacheDialog.title')}</DialogTitle>
          <DialogDescription>
            {t('settings.clearCacheDialog.body')}
          </DialogDescription>
        </DialogHeader>

        <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-2 text-sm text-text-1">
          {scan.kind === 'ok' && (
            <div className="space-y-2">
              {scan.data.categories.map((cat) => {
                const items = scan.data.items.filter(
                  (i) => i.category === cat.id,
                );
                return (
                  <div
                    key={cat.id}
                    className="rounded-lg border border-border bg-bg-1/60 p-3"
                  >
                    <div className="mb-2 flex items-center justify-between font-semibold text-text-0">
                      <span>{cat.label}</span>
                      <span className="font-mono text-xs text-text-2">
                        {humanBytes(cat.bytes)} ·{' '}
                        {t('settings.cacheFilesSuffix', {
                          count: cat.fileCount,
                        })}
                      </span>
                    </div>
                    {items.length === 0 ? (
                      <div className="text-xs text-text-2">—</div>
                    ) : (
                      <ul className="space-y-0.5 font-mono text-xs text-text-1">
                        {items.map((it, i) => (
                          <li
                            key={i}
                            className="break-all"
                          >{`・ ${it.path}  (${humanBytes(it.bytes)})`}</li>
                        ))}
                      </ul>
                    )}
                  </div>
                );
              })}
              {scan.data.truncated && (
                <div className="text-xs text-amber-500">
                  {t('settings.clearCacheDialog.moreItems', {
                    n: Math.max(
                      0,
                      scan.data.totalFiles - scan.data.items.length,
                    ),
                  })}
                </div>
              )}
            </div>
          )}
          <p className="text-xs text-text-2">
            {t('settings.clearCacheDialog.notes')}
          </p>
          {clearingError && (
            <p className="text-xs text-danger">{clearingError}</p>
          )}
        </div>

        <DialogFooter className="flex-shrink-0 border-t border-border pt-4">
          <Button variant="outline" disabled={clearing} onClick={onCancel}>
            {t('settings.clearCacheDialog.cancel')}
          </Button>
          <Button variant="danger" disabled={clearing} onClick={onConfirm}>
            {clearing ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Trash2 className="h-4 w-4" />
            )}
            {clearing
              ? t('settings.clearCacheDialog.clearing')
              : t('settings.clearCacheDialog.confirm')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
