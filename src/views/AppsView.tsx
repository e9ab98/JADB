import { useCallback, useEffect, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import { Loader2, PackagePlus, Power, RotateCcw, Smartphone, Terminal } from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  adbInstallApks,
  adbReboot,
  adbShutdown,
  type DeviceSystemInfo,
} from '@/ipc/adb';
import { usePackagesStore } from '@/store/packages';
import { useLicenseStore } from '@/store/license';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { AdbAppsTab } from '@/features/adb/AdbAppsTab';
import { AdbShellTab } from '@/features/adb/AdbShellTab';
import { FileManagerTab } from '@/features/adb/FileManagerTab';
import { AdbSystemInfoTab } from '@/features/adb/AdbSystemInfoTab';
import { LogcatTab } from '@/features/adb/LogcatTab';

/**
 * The Apps view is rendered inside its own OS-level window (see
 * `open_apps_window` Rust command), one per device serial. The window is
 * always opened with a non-empty `?serial=`, so this view always has a
 * serial to work with; the empty-state path is unreachable in practice and
 * serves only as a defensive fallback.
 */
export function AppsView() {
  const { t } = useTranslation();
  const [searchParams] = useSearchParams();
  const serial = searchParams.get('serial');
  const [installing, setInstalling] = useState(false);
  const [refreshKey, setRefreshKey] = useState(0);

  // System info cache lives in the parent so the user can switch away
  // to the Apps / Files / Shell tabs and back without re-fetching ~20
  // adb shell calls. The cache is keyed on `serial`: a different
  // device triggers a fresh fetch.
  const [systemInfo, setSystemInfo] = useState<DeviceSystemInfo | null>(null);
  const [systemInfoLoading, setSystemInfoLoading] = useState(false);
  const [systemInfoError, setSystemInfoError] = useState<string | null>(null);
  const [systemInfoUpdated, setSystemInfoUpdated] = useState<number | null>(null);

  // Monotonic request counter. Each refresh captures its own id; if
  // a newer refresh starts before this one resolves we discard the
  // stale result so a slow first fetch can't clobber a fast second one.
  const requestIdRef = useRef(0);
  const ensureSystemInfoLoaded = usePackagesStore((s) => s.ensureSystemInfoLoaded);
  const refreshSystemInfo = useCallback(async () => {
    if (!serial) return;
    const myId = ++requestIdRef.current;
    setSystemInfoLoading(true);
    // Clear stale error so a previous failure doesn't sit next to the
    // new "loading" state during the in-flight refresh.
    setSystemInfoError(null);
    try {
      // Cached by serial (60s TTL): subsequent tab switches return
      // instantly without re-running any adb commands. `adbSystemInfoViaAgent`
      // is the fast path (agent Binder + shell fallback); it falls back
      // to shell-only if the agent can't run.
      await ensureSystemInfoLoaded(serial);
      const data = usePackagesStore.getState().systemInfoBySerial[serial] ?? null;
      if (myId !== requestIdRef.current) return; // stale
      setSystemInfo(data);
      setSystemInfoUpdated(Date.now());
    } catch (e) {
      if (myId !== requestIdRef.current) return; // stale
      setSystemInfoError(String(e));
    } finally {
      if (myId === requestIdRef.current) {
        setSystemInfoLoading(false);
      }
    }
  }, [serial]);

  // Drop the cache when the device serial changes so the next mount of
  // the System tab shows fresh data instead of the previous device's.
  // Bump the request counter so any in-flight fetch for the OLD
  // serial won't write into the NEW serial's state.
  useEffect(() => {
    requestIdRef.current += 1;
    setSystemInfo(null);
    setSystemInfoError(null);
    setSystemInfoUpdated(null);
    setSystemInfoLoading(false);
    if (serial) void refreshSystemInfo();
    // refreshSystemInfo intentionally omitted from deps — it's
    // referentially stable via useCallback([serial]).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serial]);

  // Power actions state lives at the top of the view because there is only
  // one device per window — no need to namespace by serial like the
  // multi-row AdbConnectionPanel does.
  const [powerBusy, setPowerBusy] = useState<PowerAction | null>(null);

  async function doPower(action: PowerAction) {
    if (!serial || powerBusy) return;
    setPowerBusy(action);
    try {
      const verb =
        action === 'reboot' ? 'Rebooted'
        : action === 'recovery' ? 'Rebooted to recovery'
        : action === 'bootloader' ? 'Rebooted to bootloader'
        : 'Powered off';
      if (action === 'shutdown') {
        await adbShutdown(serial);
      } else {
        const mode =
          action === 'recovery' ? 'recovery'
          : action === 'bootloader' ? 'bootloader'
          : null;
        await adbReboot(serial, mode);
      }
      toast.success(`${verb} ${serial}`);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setPowerBusy(null);
    }
  }

  async function installApks() {
    if (!serial || installing) return;
    const picked = await open({ multiple: true, filters: [{ name: 'APK', extensions: ['apk'] }] });
    const paths = typeof picked === 'string' ? [picked] : picked;
    if (!paths || paths.length === 0) return;
    if (paths.length > 1 && !useLicenseStore.getState().requireFeature('adb_batch_install')) return;
    setInstalling(true);
    try {
      const result = await adbInstallApks(serial, paths);
      if (result.failed === 0) toast.success(t('adb.installSuccess', { count: result.succeeded }));
      else {
        toast.error(t('adb.installPartial', { success: result.succeeded, failed: result.failed }));
        result.items.filter((item) => !item.success).forEach((item) => toast.error(`${item.path.split(/[\\/]/).pop()}: ${item.message}`));
      }
      if (result.succeeded > 0) setRefreshKey((value) => value + 1);
    } catch (error) {
      const message = String(error);
      if (message.includes('VIP_REQUIRED:adb_batch_install')) useLicenseStore.getState().requireFeature('adb_batch_install');
      else toast.error(message);
    } finally { setInstalling(false); }
  }

  if (!serial) {
    return (
      <div className="grid h-screen w-screen place-items-center bg-bg-0 text-text-0">
        <div className="space-y-2 text-center">
          <div className="text-sm font-semibold text-text-0">
            {t('apps.noSerialTitle')}
          </div>
          <p className="text-xs text-text-2">{t('apps.noSerialDesc')}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-screen w-screen flex-col bg-bg-0 text-text-0">
      <header className="flex flex-wrap items-center justify-between gap-3 border-b border-border bg-bg-0 px-6 py-3">
        <div className="flex items-center gap-3">
          <h1 className="text-xl font-semibold text-text-0">
            {t('apps.title')}
          </h1>
          <span className="inline-flex items-center gap-1.5 rounded-md border border-border bg-bg-1 px-2 py-1 font-mono text-xs text-text-1">
            <Smartphone className="h-3 w-3 text-text-2" />
            {serial}
          </span>
        </div>
        <div className="flex items-center gap-2">
          <DevicePowerMenu
            serial={serial}
            powerBusy={powerBusy}
            onPower={doPower}
          />
          <Button onClick={() => void installApks()} disabled={installing}>
            {installing ? <Loader2 className="h-4 w-4 animate-spin" /> : <PackagePlus className="h-4 w-4" />}
            {installing ? t('adb.installing') : t('adb.installApk')}
          </Button>
        </div>
      </header>

      <main className="flex-1 overflow-auto p-6">
        <Tabs defaultValue="system">
          <TabsList>
            <TabsTrigger value="system">{t('adb.systemTab')}</TabsTrigger>
            <TabsTrigger value="apps">{t('adb.appsTab')}</TabsTrigger>
            <TabsTrigger value="files">{t('adb.filesTab')}</TabsTrigger>
            <TabsTrigger value="logs">
              <Terminal className="mr-1 h-3.5 w-3.5" />
              {t('adb.logsTab')}
            </TabsTrigger>
            <TabsTrigger value="shell">{t('adb.shellTab')}</TabsTrigger>
          </TabsList>
          <TabsContent value="system" >
            <AdbSystemInfoTab
              serial={serial}
              info={systemInfo}
              loading={systemInfoLoading}
              error={systemInfoError}
              lastUpdated={systemInfoUpdated}
              onRefresh={() => void refreshSystemInfo()}
            />
          </TabsContent>
          <TabsContent value="apps" >
            <AdbAppsTab key={`${serial}-${refreshKey}`} serial={serial} />
          </TabsContent>
          <TabsContent value="files" >
            <FileManagerTab key={serial} serial={serial} rootPath="/" />
          </TabsContent>
          <TabsContent value="logs" >
            <LogcatTab key={serial} serial={serial} />
          </TabsContent>
          <TabsContent value="shell" >
            <AdbShellTab key={serial} serial={serial} />
          </TabsContent>
        </Tabs>
      </main>
    </div>
  );
}


type PowerAction = 'reboot' | 'recovery' | 'bootloader' | 'shutdown';

function DevicePowerMenu({
  serial,
  powerBusy,
  onPower,
}: {
  serial: string;
  powerBusy: PowerAction | null;
  onPower: (action: PowerAction) => void;
}) {
  const { t } = useTranslation();
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          size="icon"
          variant="outline"
          disabled={powerBusy !== null}
          title={t('adb.powerMenuTooltip')}
          aria-label={t('adb.powerMenuTooltip')}
        >
          {powerBusy !== null ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <Power className="h-4 w-4" />
          )}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuLabel>{serial}</DropdownMenuLabel>
        <DropdownMenuSeparator />
        {([
          { action: 'reboot' as const, label: t('adb.powerReboot') },
          { action: 'recovery' as const, label: t('adb.powerRecovery') },
          { action: 'bootloader' as const, label: t('adb.powerBootloader') },
        ]).map((it) => {
          const busy = powerBusy === it.action;
          return (
            <DropdownMenuItem
              key={it.action}
              disabled={powerBusy !== null}
              onSelect={() => onPower(it.action)}
            >
              {busy ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <RotateCcw className="h-4 w-4" />
              )}
              {it.label}
            </DropdownMenuItem>
          );
        })}
        <DropdownMenuSeparator />
        <DropdownMenuItem
          disabled={powerBusy !== null}
          onSelect={() => onPower('shutdown')}
          className="text-danger data-[highlighted]:text-danger"
        >
          {powerBusy === 'shutdown' ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <Power className="h-4 w-4" />
          )}
          {t('adb.powerShutdown')}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
