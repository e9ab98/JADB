import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { toast } from 'sonner';
import {
  CheckCircle2,
  Download,
  Eraser,
  Filter,
  FolderOpen,
  Loader2,
  Package as PackageIcon,
  Play,
  Power,
  RefreshCw,
  Search,
  Smartphone,
  Trash2,
} from 'lucide-react';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Switch } from '@/components/ui/switch';
import {
  adbAppIcon,
  adbAppInfo,
  adbClearCache,
  adbExportApks,
  adbForceStop,
  adbLaunchApp,
  adbListPackages,
  adbUninstall,
  isDeviceRooted,
  listRemoteDir,
  resolveAppDataDir,
  type AppInfo,
} from '@/ipc/adb';
import { openDataDirWindow } from '@/ipc/window';
import { cn } from '@/lib/utils';

type Props = {
  serial: string | null;
};

type AppRow = AppInfo & { iconAttempted: boolean; iconLoading: boolean };
type AppCardAction = 'launch' | 'stop' | 'clear' | 'export';

function matchesQuery(app: AppRow, q: string): boolean {
  if (!q) return true;
  const needle = q.toLowerCase();
  return (
    app.packageName.toLowerCase().includes(needle) ||
    (app.appLabel?.toLowerCase().includes(needle) ?? false)
  );
}

export function AdbAppsTab({ serial }: Props) {
  const { t } = useTranslation();
  const [includeSystem, setIncludeSystem] = useState(false);
  const [query, setQuery] = useState('');
  const [loadingList, setLoadingList] = useState(false);
  const [apps, setApps] = useState<Map<string, AppRow>>(new Map());
  const [listError, setListError] = useState<string | null>(null);
  const [pendingUninstall, setPendingUninstall] = useState<AppRow | null>(null);
  const [uninstalling, setUninstalling] = useState(false);
  const [infoLoading, setInfoLoading] = useState<Set<string>>(new Set());
  // Per-device cache of `is_device_rooted` so we don't re-probe su on every
  // release-package click. Re-populated on demand; cleared on unmount.
  const [rootStatus, setRootStatus] = useState<Record<string, boolean>>({});
  const [pendingClear, setPendingClear] = useState<AppRow | null>(null);
  // Per-row in-flight destructive action. Key = packageName, value = which
  // action is running (lets us show a spinner on the right button).
  const [busyAction, setBusyAction] = useState<Map<string, AppCardAction>>(
    new Map(),
  );

  function setRowAction(pkg: string, action: AppCardAction | null) {
    setBusyAction((prev) => {
      const next = new Map(prev);
      if (action) next.set(pkg, action);
      else next.delete(pkg);
      return next;
    });
  }
  const appsRef = useRef(apps);
  appsRef.current = apps;
  const iconAttemptedRef = useRef<Set<string>>(new Set());

  // Limit concurrent adb calls so a 200-app device doesn't lock up the host.
  const ICON_CONCURRENCY = 6;
  const INFO_CONCURRENCY = 4;
  const inFlightRef = useRef<Set<string>>(new Set());

  const loadPackages = useCallback(async () => {
    if (!serial) return;
    setLoadingList(true);
    setListError(null);
    try {
      const list = await adbListPackages(serial, includeSystem);
      setApps((prev) => {
        const next = new Map<string, AppRow>();
        for (const pkg of list) {
          const existing = prev.get(pkg);
          next.set(pkg, existing ?? {
            packageName: pkg,
            appLabel: null,
            versionName: null,
            versionCode: null,
            minSdk: null,
            targetSdk: null,
            apkPath: null,
            iconPath: null,
            iconDataUrl: null,
            iconAttempted: false,
            isSystem: includeSystem ? false : true,
            isDebuggable: false,
            iconLoading: false,
          });
        }
        return next;
      });
    } catch (e) {
      const msg = String(e);
      setListError(msg);
      toast.error(msg);
    } finally {
      setLoadingList(false);
    }
  }, [serial, includeSystem]);

  // When the device changes, drop everything.
  useEffect(() => {
    setApps(new Map());
    setQuery('');
    setInfoLoading(new Set());
    inFlightRef.current.clear();
    iconAttemptedRef.current.clear();
    if (serial) void loadPackages();
  }, [serial, loadPackages]);

  // Bounded-concurrency worker: hydrate info + icons for packages that don't
  // have it yet. We stop early once the user navigates away (via inFlightRef).
  useEffect(() => {
    if (!serial) return;
    let cancelled = false;
    const queue: string[] = [];

    const infoSlots = Array.from({ length: INFO_CONCURRENCY }, () => ({
      busy: false,
      current: null as string | null,
    }));
    const iconSlots = Array.from({ length: ICON_CONCURRENCY }, () => ({
      busy: false,
      current: null as string | null,
    }));

    function refill() {
      appsRef.current.forEach((row, pkg) => {
        const needsInfo =
          row.appLabel === null &&
          row.versionName === null &&
          row.versionCode === null &&
          !inFlightRef.current.has(`info:${pkg}`);
        const needsIcon =
          row.iconDataUrl === null &&
          !row.iconAttempted &&
          !iconAttemptedRef.current.has(pkg) &&
          !row.iconLoading;
        if (needsInfo || needsIcon) {
          if (!queue.includes(pkg)) queue.push(pkg);
        }
      });
    }

    async function pumpInfo(slot: (typeof infoSlots)[number]) {
      while (!cancelled) {
        if (slot.busy) {
          await new Promise((r) => setTimeout(r, 50));
          continue;
        }
        const pkg = queue.shift();
        if (!pkg) {
          await new Promise((r) => setTimeout(r, 200));
          continue;
        }
        slot.busy = true;
        slot.current = pkg;
        inFlightRef.current.add(`info:${pkg}`);
        setInfoLoading((s) => new Set(s).add(pkg));
        try {
          const info = await adbAppInfo(serial!, pkg);
          if (cancelled) return;
          setApps((prev) => {
            const next = new Map(prev);
            const existing = next.get(pkg);
            next.set(pkg, {
              ...(existing ?? {
                packageName: pkg,
                appLabel: null,
                versionName: null,
                versionCode: null,
                minSdk: null,
                targetSdk: null,
                apkPath: null,
                iconPath: null,
                iconDataUrl: null,
                iconAttempted: false,
                isSystem: false,
                isDebuggable: false,
                iconLoading: false,
              }),
              appLabel: info.appLabel ?? existing?.appLabel ?? null,
              versionName: info.versionName ?? existing?.versionName ?? null,
              versionCode: info.versionCode ?? existing?.versionCode ?? null,
              minSdk: info.minSdk ?? existing?.minSdk ?? null,
              targetSdk: info.targetSdk ?? existing?.targetSdk ?? null,
              apkPath: info.apkPath ?? existing?.apkPath ?? null,
              iconPath: info.iconPath ?? existing?.iconPath ?? null,
              isSystem: info.isSystem,
              isDebuggable: info.isDebuggable,
            });
            return next;
          });
        } catch {
          // Non-fatal: a single broken package shouldn't block the rest.
        } finally {
          inFlightRef.current.delete(`info:${pkg}`);
          slot.busy = false;
          slot.current = null;
          setInfoLoading((s) => {
            const next = new Set(s);
            next.delete(pkg);
            return next;
          });
        }
      }
    }

    async function pumpIcon(slot: (typeof iconSlots)[number]) {
      while (!cancelled) {
        if (slot.busy) {
          await new Promise((r) => setTimeout(r, 50));
          continue;
        }
        // Prefer packages we already have info for, then fall back to any.
        const candidate =
          Array.from(appsRef.current.entries()).find(
            ([pkg, row]) =>
              row.iconDataUrl === null &&
              !row.iconAttempted &&
              !iconAttemptedRef.current.has(pkg) &&
              !row.iconLoading &&
              !inFlightRef.current.has(`icon:${pkg}`) &&
              (row.appLabel ?? row.versionName) !== null,
          ) ??
          Array.from(appsRef.current.entries()).find(
            ([pkg, row]) =>
              row.iconDataUrl === null &&
              !row.iconAttempted &&
              !iconAttemptedRef.current.has(pkg) &&
              !row.iconLoading &&
              !inFlightRef.current.has(`icon:${pkg}`),
          );
        if (!candidate) {
          await new Promise((r) => setTimeout(r, 200));
          continue;
        }
        const [pkg, row] = candidate;
        slot.busy = true;
        slot.current = pkg;
        inFlightRef.current.add(`icon:${pkg}`);
        setApps((prev) => {
          const next = new Map(prev);
          if (next.has(pkg)) {
            next.set(pkg, { ...(next.get(pkg) as AppRow), iconLoading: true });
          }
          return next;
        });
        try {
          const dataUrl = await adbAppIcon(serial!, pkg);
          if (cancelled) return;
          iconAttemptedRef.current.add(pkg);
          setApps((prev) => {
            const next = new Map(prev);
            const existing = next.get(pkg);
            if (existing) {
              next.set(pkg, {
                ...existing,
                iconDataUrl: dataUrl,
                iconAttempted: true,
                iconLoading: false,
              });
            }
            return next;
          });
        } catch {
          iconAttemptedRef.current.add(pkg);
          setApps((prev) => {
            const next = new Map(prev);
            const existing = next.get(pkg);
            if (existing) {
              next.set(pkg, {
                ...existing,
                iconAttempted: true,
                iconLoading: false,
              });
            }
            return next;
          });
        } finally {
          inFlightRef.current.delete(`icon:${pkg}`);
          slot.busy = false;
          slot.current = null;
        }
        // touch row to silence unused-var lint when row is only used as a
        // discriminator above.
        void row;
      }
    }

    refill();
    const timers = [
      ...infoSlots.map((s) => pumpInfo(s)),
      ...iconSlots.map((s) => pumpIcon(s)),
    ];

    // Periodically refresh the queue so newly-arrived packages get picked up.
    const refillId = setInterval(() => {
      if (cancelled) return;
      refill();
    }, 1000);

    return () => {
      cancelled = true;
      clearInterval(refillId);
      // Best-effort wait for in-flight pumps; they'll early-return on the
      // next await because cancelled flipped.
      void Promise.all(timers);
    };
  }, [serial]);

  const filtered = useMemo(() => {
    const all = Array.from(apps.values());
    const filteredAll = all
      .filter((a) => matchesQuery(a, query.trim()))
      .sort((a, b) => {
        const la = a.appLabel ?? a.packageName;
        const lb = b.appLabel ?? b.packageName;
        return la.localeCompare(lb);
      });
    return filteredAll;
  }, [apps, query]);

  async function isRooted(s: string): Promise<boolean> {
    if (s in rootStatus) return rootStatus[s] === true;
    try {
      const rooted = await isDeviceRooted(s);
      if (rooted) {
        setRootStatus((prev) => ({ ...prev, [s]: true }));
      }
      return rooted;
    } catch {
      return false;
    }
  }

  async function openDataDir(row: AppRow) {
    if (!serial) {
      toast.error(t('adb.noDeviceSelected'));
      return;
    }

    // Map a backend error string to a localized, actionable toast message.
    // Backend errors come from `adb -s <serial> shell ...` and typically read
    // like: "tool failed: adb (exit=N): <stderr>". We only look at substrings
    // that are stable across adb versions and Android shells.
    function friendlyDirError(raw: string): string {
      const lower = raw.toLowerCase();
      // Permission / no-root / run-as must come BEFORE the device and path
      // checks below so a "permission denied" or run-as debuggable probe
      // failure isn't misclassified as a dropped device or missing path.
      if (lower.includes('permission denied')) {
        return t('adb.cannotOpenDataDirPermission', {
          pkg: row.packageName,
          device: serial,
        });
      }
      if (lower.includes('not running as root')) {
        return t('adb.cannotOpenDataDirNoRoot', {
          pkg: row.packageName,
          device: serial,
        });
      }
      if (lower.includes('run-as')) {
        return t('adb.cannotOpenDataDirRunAs', {
          pkg: row.packageName,
          device: serial,
        });
      }
      // Device-level errors must come BEFORE the generic "not found" branch
      // so a dropped-device case ("device 'serial' not found", "error:
      // closed") isn't misreported as a path-level miss. The bare substring
      // "closed" is too broad (matches unrelated stderr) so we anchor on
      // the adb-specific prefix "error: closed".
      if (lower.includes('device offline')) {
        return t('adb.cannotOpenDataDirDeviceOffline', {
          pkg: row.packageName,
          device: serial,
        });
      }
      if (
        lower.includes('connection lost') ||
        lower.includes('error: closed') ||
        (lower.includes('device ') && lower.includes('not found'))
      ) {
        return t('adb.cannotOpenDataDirDeviceClosed', {
          pkg: row.packageName,
          device: serial,
        });
      }
      if (lower.includes('no such file') || lower.includes('not found')) {
        return t('adb.cannotOpenDataDirNotFound', {
          pkg: row.packageName,
          device: serial,
        });
      }
      return t('adb.cannotOpenDataDirGeneric', {
        pkg: row.packageName,
        device: serial,
        error: raw,
      });
    }

    // Pick the right execution mode up front:
    //   debug app   -> run-as <pkg>     (works on any device)
    //   release app -> su/root shell    (only works if device is rooted)
    // Root status is probed lazily and cached per-device.
    let asPkg: string | null;
    let useRoot = false;
    let path: string;
    if (row.isDebuggable) {
      asPkg = row.packageName;
      try {
        path = await resolveAppDataDir(
          serial,
          row.packageName,
          asPkg,
          false,
        );
      } catch (runAsError) {
        if (!(await isRooted(serial))) {
          toast.error(friendlyDirError(String(runAsError)));
          return;
        }
        asPkg = null;
        useRoot = true;
        try {
          path = await resolveAppDataDir(
            serial,
            row.packageName,
            null,
            true,
          );
        } catch (rootError) {
          toast.error(friendlyDirError(String(rootError)));
          return;
        }
      }
    } else {
      const rooted = await isRooted(serial);
      if (!rooted) {
        toast.error(
          t('adb.cannotOpenDataDirRelease', { pkg: row.packageName })
        );
        return;
      }
      asPkg = null;
      useRoot = true;
      try {
        path = await resolveAppDataDir(
          serial,
          row.packageName,
          null,
          true,
        );
      } catch (e) {
        toast.error(friendlyDirError(String(e)));
        return;
      }
    }

    try {
      await listRemoteDir(serial, path, asPkg, useRoot);
    } catch (e) {
      toast.error(friendlyDirError(String(e)));
      return;
    }

    try {
      await openDataDirWindow(
        serial,
        row.packageName,
        asPkg !== null,
        useRoot,
        path,
      );
      toast.success(t('adb.openDataDirSuccess', { pkg: row.packageName }));
    } catch (e) {
      toast.error(
        t('adb.openDataDirFailedGeneric', {
          device: serial,
          pkg: row.packageName,
          error: String(e),
        }),
      );
    }
  }

  async function forceStop(row: AppRow) {
    if (!serial) return;
    const pkg = row.packageName;
    setRowAction(pkg, 'stop');
    try {
      const out = await adbForceStop(serial, pkg);
      toast.success(out || t('adb.forceStopSuccess', { pkg }));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setRowAction(pkg, null);
    }
  }

  async function launchApp(row: AppRow) {
    if (!serial) return;
    const pkg = row.packageName;
    setRowAction(pkg, 'launch');
    try {
      await adbLaunchApp(serial, pkg);
      toast.success(t('adb.launchAppSuccess', { pkg }));
    } catch (e) {
      const error = String(e);
      if (error.toLowerCase().includes('no launcher activity found')) {
        toast.error(t('adb.launchAppNoActivity', { pkg }));
      } else {
        toast.error(error);
      }
    } finally {
      setRowAction(pkg, null);
    }
  }

  async function confirmClearCache() {
    if (!pendingClear || !serial) return;
    const pkg = pendingClear.packageName;
    setRowAction(pkg, 'clear');
    try {
      const out = await adbClearCache(serial, pkg);
      toast.success(out || t('adb.clearCacheSuccess', { pkg }));
      setPendingClear(null);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setRowAction(pkg, null);
    }
  }

  async function exportApks(row: AppRow) {
    if (!serial) return;
    const pkg = row.packageName;
    setRowAction(pkg, 'export');
    try {
      const picked = await openDialog({
        directory: true,
        multiple: false,
        title: t('adb.exportApks'),
      });
      if (!picked || Array.isArray(picked)) return;
      const result = await adbExportApks(
        serial,
        pkg,
        row.versionName,
        picked,
      );
      toast.success(
        t('adb.exportApksSuccess', {
          count: result.count,
          directory: result.directory,
        }),
      );
    } catch (error) {
      toast.error(
        t('adb.exportApksFailed', {
          pkg,
          error: String(error),
        }),
      );
    } finally {
      setRowAction(pkg, null);
    }
  }

  async function confirmUninstall() {
    if (!pendingUninstall || !serial) return;
    const pkg = pendingUninstall.packageName;
    setUninstalling(true);
    try {
      const out = await adbUninstall(serial, pkg);
      toast.success(out || t('adb.uninstallSuccess', { pkg }));
      setApps((prev) => {
        const next = new Map(prev);
        next.delete(pkg);
        return next;
      });
      setPendingUninstall(null);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setUninstalling(false);
    }
  }

  if (!serial) {
    return (
      <Card>
        <CardContent className="flex items-center gap-3 text-sm text-text-2">
          <Smartphone className="h-4 w-4" />
          {t('adb.noDeviceSelected')}
        </CardContent>
      </Card>
    );
  }

  return (
    <div className="space-y-4">
      <Card>
        <CardContent className="flex flex-wrap items-center gap-3">
          <div className="flex items-center gap-2 text-sm font-medium text-text-0">
            <Filter className="h-4 w-4 text-text-1" />
            {t('adb.appsTitle')}
          </div>
          <Badge variant="secondary">
            {filtered.length} {t('adb.apps')}
          </Badge>
          <div className="ml-auto flex flex-wrap items-center gap-3">
            <label className="flex items-center gap-2 text-xs text-text-2">
              <Switch
                checked={includeSystem}
                onCheckedChange={(v) => setIncludeSystem(Boolean(v))}
              />
              {t('adb.includeSystem')}
            </label>
            <Button
              size="sm"
              variant="outline"
              onClick={() => void loadPackages()}
              disabled={loadingList}
            >
              {loadingList ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <RefreshCw className="h-4 w-4" />
              )}
              {t('common.refresh')}
            </Button>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardContent className="flex items-center gap-2">
          <Search className="h-4 w-4 text-text-2" />
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t('adb.searchPlaceholder')}
            className="flex-1"
          />
        </CardContent>
      </Card>

      {listError && (
        <Card className="border-danger">
          <CardContent className="text-sm text-danger">{listError}</CardContent>
        </Card>
      )}

      {loadingList && filtered.length === 0 ? (
        <Card>
          <CardContent className="flex items-center gap-3 text-sm text-text-2">
            <Loader2 className="h-4 w-4 animate-spin text-brand" />
            {t('adb.loadingApps')}
          </CardContent>
        </Card>
      ) : filtered.length === 0 ? (
        <Card>
          <CardContent className="text-sm text-text-2">
            {t('adb.noApps')}
          </CardContent>
        </Card>
      ) : (
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
          {filtered.map((row) => (
            <AppCard
              key={row.packageName}
              app={row}
              infoLoading={infoLoading.has(row.packageName)}
              busyAction={busyAction.get(row.packageName) ?? null}
              onLaunch={() => void launchApp(row)}
              onForceStop={() => void forceStop(row)}
              onClearCache={() => setPendingClear(row)}
              onOpenDataDir={() => void openDataDir(row)}
              onExportApks={() => void exportApks(row)}
              onUninstall={() => setPendingUninstall(row)}
            />
          ))}
        </div>
      )}

      <Dialog
        open={pendingUninstall !== null}
        onOpenChange={(open) => {
          if (!open && !uninstalling) setPendingUninstall(null);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('adb.uninstallConfirmTitle')}</DialogTitle>
            <DialogDescription>
              {pendingUninstall
                ? t('adb.uninstallConfirmDesc', {
                    label:
                      pendingUninstall.appLabel ?? pendingUninstall.packageName,
                    pkg: pendingUninstall.packageName,
                  })
                : ''}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setPendingUninstall(null)}
              disabled={uninstalling}
            >
              {t('common.cancel')}
            </Button>
            <Button
              variant="danger"
              onClick={() => void confirmUninstall()}
              disabled={uninstalling}
            >
              {uninstalling && <Loader2 className="h-4 w-4 animate-spin" />}
              <Trash2 className="h-4 w-4" />
              {t('adb.uninstall')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={pendingClear !== null}
        onOpenChange={(open) => {
          if (!open && busyAction.get(pendingClear?.packageName ?? '') !== 'clear') {
            setPendingClear(null);
          }
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('adb.clearCacheConfirmTitle')}</DialogTitle>
            <DialogDescription>
              {pendingClear
                ? t('adb.clearCacheConfirmDesc', {
                    label: pendingClear.appLabel ?? pendingClear.packageName,
                    pkg: pendingClear.packageName,
                  })
                : ''}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setPendingClear(null)}
              disabled={busyAction.get(pendingClear?.packageName ?? '') === 'clear'}
            >
              {t('common.cancel')}
            </Button>
            <Button
              variant="danger"
              onClick={() => void confirmClearCache()}
              disabled={busyAction.get(pendingClear?.packageName ?? '') === 'clear'}
            >
              {busyAction.get(pendingClear?.packageName ?? '') === 'clear' && (
                <Loader2 className="h-4 w-4 animate-spin" />
              )}
              <Eraser className="h-4 w-4" />
              {t('adb.clearCache')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function AppCard({
  app,
  infoLoading,
  busyAction,
  onLaunch,
  onForceStop,
  onClearCache,
  onOpenDataDir,
  onExportApks,
  onUninstall,
}: {
  app: AppRow;
  infoLoading: boolean;
  busyAction: AppCardAction | null;
  onLaunch: () => void;
  onForceStop: () => void;
  onClearCache: () => void;
  onOpenDataDir: () => void;
  onExportApks: () => void;
  onUninstall: () => void;
}) {
  const { t } = useTranslation();
  const displayLabel = app.appLabel ?? app.packageName;
  const showVersion =
    app.versionName !== null || app.versionCode !== null;
  return (
    <Card className="overflow-hidden">
      <CardContent className="space-y-3">
        <div className="flex items-start gap-3">
          <div className="grid h-12 w-12 shrink-0 place-items-center overflow-hidden rounded-xl border border-border bg-bg-2">
            {app.iconDataUrl ? (
              <img
                src={app.iconDataUrl}
                alt={displayLabel}
                className="h-full w-full object-cover"
              />
            ) : app.iconLoading ? (
              <Loader2 className="h-5 w-5 animate-spin text-text-2" />
            ) : (
              <PackageIcon className="h-6 w-6 text-text-2" />
            )}
          </div>
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-1">
              <div
                className={cn(
                  'truncate text-sm font-semibold text-text-0',
                  !app.appLabel && 'font-mono text-xs',
                )}
                title={displayLabel}
              >
                {infoLoading && !app.appLabel ? (
                  <span className="text-text-2">…</span>
                ) : (
                  displayLabel
                )}
              </div>
              {app.isSystem && (
                <Badge variant="outline" className="ml-1 shrink-0">
                  system
                </Badge>
              )}
              <Badge
                variant={app.isDebuggable ? 'danger' : 'success'}
                className="ml-1 shrink-0"
              >
                {app.isDebuggable
                  ? t('adb.debugBuild')
                  : t('adb.releaseBuild')}
              </Badge>
            </div>
            {app.appLabel && (
              <div
                className="truncate font-mono text-xs text-text-2"
                title={app.packageName}
              >
                {app.packageName}
              </div>
            )}
          </div>
        </div>

        <div className="space-y-1 text-xs text-text-2">
          <div className="flex items-center justify-between gap-2">
            <span>{t('adb.version')}</span>
            <span className="font-mono text-text-1">
              {showVersion
                ? `${app.versionName ?? '—'} (${app.versionCode ?? '—'})`
                : infoLoading
                ? '…'
                : '—'}
            </span>
          </div>
          <div className="flex items-center justify-between gap-2">
            <span>targetSdk</span>
            <span className="font-mono text-text-1">
              {app.targetSdk ?? (infoLoading ? '…' : '—')}
            </span>
          </div>
          {infoLoading && !app.appLabel && (
            <div className="flex items-center gap-1 text-text-2">
              <Loader2 className="h-3 w-3 animate-spin" />
              {t('adb.loadingInfo')}
            </div>
          )}
          {!infoLoading && app.iconDataUrl && (
            <div className="flex items-center gap-1 text-success">
              <CheckCircle2 className="h-3 w-3" />
              {t('adb.ready')}
            </div>
          )}
        </div>

        <div className="flex items-center justify-end gap-1 pt-1">
          <Button
            size="sm"
            variant="outline"
            onClick={onLaunch}
            disabled={busyAction !== null}
            title={t('adb.launchApp')}
          >
            {busyAction === 'launch' ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Play className="h-3 w-3" />
            )}
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={onForceStop}
            disabled={busyAction !== null}
            title={t('adb.forceStop')}
          >
            {busyAction === 'stop' ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Power className="h-3 w-3" />
            )}
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={onClearCache}
            disabled={busyAction !== null}
            title={t('adb.clearCache')}
          >
            {busyAction === 'clear' ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Eraser className="h-3 w-3" />
            )}
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={onOpenDataDir}
            disabled={busyAction !== null}
            title={t('adb.openDataDir')}
          >
            <FolderOpen className="h-3 w-3" />
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={onExportApks}
            disabled={busyAction !== null}
            title={t('adb.exportApks')}
          >
            {busyAction === 'export' ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Download className="h-3 w-3" />
            )}
          </Button>
          <Button
            size="sm"
            variant="danger"
            onClick={onUninstall}
            disabled={app.isSystem || busyAction !== null}
            title={app.isSystem ? t('adb.uninstallSystemBlocked') : t('adb.uninstall')}
          >
            <Trash2 className="h-3 w-3" />
            {t('adb.uninstall')}
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
