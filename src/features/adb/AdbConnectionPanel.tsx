import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import { toast } from 'sonner';
import {
  AlertTriangle,
  ChevronRight,
  Loader2,
  PlugZap,
  RefreshCw,
  Settings as SettingsIcon,
  Smartphone,
  Unplug,
  Wifi,
} from 'lucide-react';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import {
  adbConnect,
  adbDevices,
  adbDisconnect,
  type AdbDevice,
} from '@/ipc/adb';
import { openAppsWindow } from '@/ipc/window';
import { useSettingsStore } from '@/store/settings';
import { cn } from '@/lib/utils';

function stateTone(state: string): 'success' | 'warning' | 'danger' {
  const s = state.toLowerCase();
  if (s === 'device' || s === 'online') return 'success';
  if (s === 'unauthorized' || s === 'offline') return 'danger';
  return 'warning';
}

function isLocalSerial(serial: string): boolean {
  // Local devices use USB serials / emulator-* ids; remote devices come back
  // from `adb connect` as `<host>:<port>`.
  return !serial.includes(':') || serial.startsWith('emulator-');
}

export function AdbConnectionPanel() {
  const { t } = useTranslation();
  const adbPath = useSettingsStore((s) => s.settings?.adbPath ?? null);
  const [devices, setDevices] = useState<AdbDevice[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [host, setHost] = useState('');
  const [port, setPort] = useState('5555');
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [openingSerial, setOpeningSerial] = useState<string | null>(null);

  async function refresh() {
    if (!adbPath) {
      setDevices([]);
      setError(null);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const list = await adbDevices();
      setDevices(list);
    } catch (e) {
      const msg = String(e);
      setError(msg);
      setDevices([]);
      toast.error(msg);
    } finally {
      setLoading(false);
    }
  }

  // Initial load + lightweight polling while we're on this tab. 5 s is enough
  // to catch a freshly-attached USB device without spamming adb.
  useEffect(() => {
    void refresh();
    const id = setInterval(() => {
      void refresh();
    }, 5000);
    return () => clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [adbPath]);

  async function doConnect() {
    if (!host.trim()) {
      toast.error(t('adb.hostRequired'));
      return;
    }
    const portNum = Number.parseInt(port, 10);
    if (!Number.isInteger(portNum) || portNum <= 0 || portNum > 65535) {
      toast.error(t('adb.portInvalid'));
      return;
    }
    setBusyAction('connect');
    try {
      const out = await adbConnect(host.trim(), portNum);
      toast.success(out || t('adb.connected'));
      await refresh();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusyAction(null);
    }
  }

  async function doDisconnect(target?: string) {
    setBusyAction(target ? `disconnect:${target}` : 'disconnect:all');
    try {
      const out = await adbDisconnect(target ?? null);
      toast.success(out || t('adb.disconnected'));
      await refresh();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusyAction(null);
    }
  }

  async function openDeviceApps(serial: string) {
    setOpeningSerial(serial);
    try {
      await openAppsWindow(serial);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setOpeningSerial(null);
    }
  }

  if (!adbPath) {
    return <AdbNotConfiguredCard />;
  }

  const onlineDevices = devices.filter((d) => d.state === 'device');
  const offlineDevices = devices.filter((d) => d.state !== 'device');

  return (
    <div className="space-y-4">
      <Card>
        <CardContent className="flex flex-wrap items-center gap-3">
          <div className="flex items-center gap-2 text-sm font-semibold text-text-0">
            <Wifi className="h-4 w-4 text-brand" />
            {t('adb.connectionTitle')}
          </div>
          <div className="ml-2 flex items-center gap-2 text-xs text-text-2">
            {loading ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Badge variant="secondary">
                {t('adb.deviceCount', {
                  online: onlineDevices.length,
                  total: devices.length,
                })}
              </Badge>
            )}
          </div>
          <div className="ml-auto flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={() => void refresh()}
              disabled={loading}
            >
              <RefreshCw
                className={cn('h-4 w-4', loading && 'animate-spin')}
              />
              {t('common.refresh')}
            </Button>
          </div>
        </CardContent>
      </Card>

      {error && (
        <Card className="border-danger">
          <CardContent className="flex items-start gap-3 text-sm">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-danger" />
            <div className="min-w-0 space-y-1">
              <div className="font-semibold text-text-0">
                {t('adb.errorTitle')}
              </div>
              <div className="break-all font-mono text-xs text-text-2">
                {error}
              </div>
            </div>
          </CardContent>
        </Card>
      )}

      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardContent className="space-y-3">
            <div className="flex items-center gap-2 text-sm font-medium text-text-0">
              <Smartphone className="h-4 w-4 text-text-1" />
              {t('adb.localDevices')}
            </div>
            <p className="text-xs text-text-2">{t('adb.localHint')}</p>
            <div className="space-y-2">
              {devices.length === 0 ? (
                <div className="rounded-md border border-dashed border-border p-3 text-xs text-text-2">
                  {loading ? t('adb.scanning') : t('adb.noDevices')}
                </div>
              ) : (
                devices.map((d) => (
                  <DeviceRow
                    key={d.serial}
                    device={d}
                    opening={openingSerial === d.serial}
                    onOpen={() => void openDeviceApps(d.serial)}
                  />
                ))
              )}
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="space-y-3">
            <div className="flex items-center gap-2 text-sm font-medium text-text-0">
              <PlugZap className="h-4 w-4 text-text-1" />
              {t('adb.ipConnect')}
            </div>
            <p className="text-xs text-text-2">{t('adb.ipHint')}</p>
            <div className="flex flex-wrap items-center gap-2">
              <Input
                placeholder={t('adb.hostPlaceholder')}
                value={host}
                onChange={(e) => setHost(e.target.value)}
                className="flex-1 min-w-[160px]"
              />
              <Input
                placeholder={t('adb.portPlaceholder')}
                value={port}
                onChange={(e) => setPort(e.target.value.replace(/[^\d]/g, ''))}
                className="w-24"
                inputMode="numeric"
              />
              <Button
                onClick={() => void doConnect()}
                disabled={busyAction === 'connect'}
              >
                {busyAction === 'connect' && (
                  <Loader2 className="h-4 w-4 animate-spin" />
                )}
                {t('adb.connect')}
              </Button>
            </div>

            {offlineDevices.length > 0 && (
              <div className="space-y-1 pt-2">
                <div className="text-xs font-medium text-text-2">
                  {t('adb.offlineDevices')}
                </div>
                <div className="space-y-1">
                  {offlineDevices.map((d) => (
                    <div
                      key={d.serial}
                      className="flex items-center justify-between gap-2 rounded-md border border-border bg-bg-1 px-3 py-2 text-xs"
                    >
                      <span className="font-mono text-text-1">{d.serial}</span>
                      <div className="flex items-center gap-2">
                        <Badge variant={stateTone(d.state)}>{d.state}</Badge>
                        {!isLocalSerial(d.serial) && (
                          <Button
                            size="sm"
                            variant="ghost"
                            onClick={() => void doDisconnect(d.serial)}
                            disabled={busyAction === `disconnect:${d.serial}`}
                            title={t('adb.disconnect')}
                          >
                            <Unplug className="h-3 w-3" />
                          </Button>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
                <div>
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => void doDisconnect()}
                    disabled={busyAction === 'disconnect:all'}
                    className="mt-2"
                  >
                    {busyAction === 'disconnect:all' && (
                      <Loader2 className="h-3 w-3 animate-spin" />
                    )}
                    {t('adb.disconnectAll')}
                  </Button>
                </div>
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

function DeviceRow({
  device,
  opening,
  onOpen,
}: {
  device: AdbDevice;
  opening: boolean;
  onOpen: () => void;
}) {
  const { t } = useTranslation();
  const tone = stateTone(device.state);
  const canOpen = device.state === 'device';
  const disabled = opening;
  return (
    <div
      role="button"
      tabIndex={canOpen && !opening ? 0 : -1}
      onClick={canOpen && !opening ? onOpen : undefined}
      onKeyDown={
        canOpen && !opening
          ? (e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                onOpen();
              }
            }
          : undefined
      }
      className={cn(
        'flex w-full items-center justify-between gap-2 rounded-md border bg-bg-1 px-3 py-2 text-left text-sm transition-colors',
        'border-border',
        canOpen && !opening && 'cursor-pointer hover:border-border-strong hover:bg-bg-2',
        !canOpen && 'cursor-not-allowed opacity-60',
      )}
    >
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <Smartphone
            className={cn(
              'h-4 w-4 shrink-0',
              canOpen ? 'text-brand' : 'text-text-2',
            )}
          />
          <span className="truncate font-mono text-xs text-text-0">
            {device.serial}
          </span>
        </div>
        <div className="mt-0.5 truncate text-xs text-text-2">
          {device.model ?? device.product ?? t('adb.unknownDevice')}
        </div>
      </div>
      <div className="flex items-center gap-2">
        <Badge variant={tone}>{device.state}</Badge>
        {canOpen && (
          <Button
            size="sm"
            variant="ghost"
            onClick={(e) => {
              e.stopPropagation();
              onOpen();
            }}
            disabled={disabled}
            title={t('adb.openApps')}
          >
            {opening ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <>
                {t('adb.openApps')}
                <ChevronRight className="h-3 w-3" />
              </>
            )}
          </Button>
        )}
      </div>
    </div>
  );
}

/**
 * Empty-state CTA shown when the user lands on the ADB tab without an adb
 * binary configured. We deep-link into Settings → Tools so they can either
 * download adb or point at an existing local binary.
 */
function AdbNotConfiguredCard() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  return (
    <Card className="border-warning">
      <CardContent className="flex flex-wrap items-start gap-4 text-sm">
        <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-warning" />
        <div className="min-w-0 flex-1 space-y-2">
          <div className="font-semibold text-text-0">
            {t('adb.adbMissingTitle')}
          </div>
          <p className="text-text-2">{t('adb.adbMissingDesc')}</p>
        </div>
        <Button
          onClick={() => navigate('/settings?tab=tools')}
          className="shrink-0"
        >
          <SettingsIcon className="h-4 w-4" />
          {t('adb.adbMissingCta')}
        </Button>
      </CardContent>
    </Card>
  );
}
