import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import {
  AlertTriangle,
  Loader2,
  RefreshCw,
  Wrench,
} from 'lucide-react';
import { toast } from 'sonner';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { adbDevices, type AdbDevice } from '@/ipc/adb';
import { useSettingsStore } from '@/store/settings';
import { cn } from '@/lib/utils';
import { MiuiUsbInstallCard } from './MiuiUsbInstallCard';
import { ScreenshotCard } from './ScreenshotCard';
import { WirelessTcpipCard } from './WirelessTcpipCard';
import { DevOptionsCard } from './DevOptionsCard';
import { FreezeAppsCard } from './FreezeAppsCard';
import { BugReportCard } from './BugReportCard';

/**
 * Tools tab content. A "catch-all" for one-off helper actions that
 * don't deserve their own route -- each tool is a self-contained
 * card. The grid is intentionally `grid-cols-1 md:2 xl:3` so the
 * first 1-3 cards stay above the fold on a laptop while longer
 * lists remain scannable.
 *
 * We pull the device list the same way the Fastboot/Recovery panels
 * do (5 s poll) so a freshly-attached USB device shows up without a
 * manual reload. We deliberately don't reuse the ADB panel's
 * selection state -- tools tend to run against a specific device
 * and the user should be able to flip between serials without a
 * cross-tab reshuffle.
 *
 * The `WirelessTcpipCard` needs a selected serial (the user picks
 * which device to open the TCP port on) but it sits above the
 * device-required grid because, semantically, it's the first step
 * of "stop using USB".
 */
export function ToolsPanel() {
  const { t } = useTranslation();
  const adbPath = useSettingsStore((s) => s.settings?.adbPath ?? null);
  const [devices, setDevices] = useState<AdbDevice[]>([]);
  const [loading, setLoading] = useState(false);
  // The selected serial. Defaults to the first online device; the
  // user can override via the dropdown.
  const [selected, setSelected] = useState<string>('');
  // Bumped after a successful wireless pair so cards depending on
  // a connected device (most of them) re-fetch their state from
  // the freshly-added serial.

  const online = devices.filter((d) => d.state === 'device');

  async function refresh() {
    if (!adbPath) {
      setDevices([]);
      return;
    }
    setLoading(true);
    try {
      const list = await adbDevices();
      setDevices(list);
    } catch (e) {
      toast.error(String(e));
      setDevices([]);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void refresh();
    const id = setInterval(() => void refresh(), 5000);
    return () => clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [adbPath]);

  // Keep the selection valid -- if the active device disappears (USB
  // unplug, wireless drop) fall back to the first online device.
  useEffect(() => {
    if (!online.length) {
      if (selected) setSelected('');
      return;
    }
    if (!selected || !online.some((d) => d.serial === selected)) {
      // `online[0]` is reachable here because `online.length > 0`
      // (the early-return above handled the empty case); spell the
      // fact out so `noUncheckedIndexedAccess` doesn't barf on the
      // `T | undefined` index type.
      const first = online[0];
      if (first) setSelected(first.serial);
    }
  }, [online, selected]);

  if (!adbPath) {
    return (
      <Card>
        <CardContent className="flex items-center gap-3 text-sm text-text-2">
          <AlertTriangle className="h-4 w-4 text-warning" />
          {t('adb.adbMissingDesc')}
        </CardContent>
      </Card>
    );
  }

  return (
    <div className="space-y-4">
      <Card>
        <CardContent className="flex flex-wrap items-center gap-3">
          <div className="flex items-center gap-2 text-sm font-semibold text-text-0">
            <Wrench className="h-4 w-4 text-brand" />
            {t('tools.title')}
          </div>
          <div className="ml-2 flex items-center gap-2 text-xs text-text-2">
            {loading ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Badge variant="secondary">
                {t('tools.deviceCount', { count: online.length })}
              </Badge>
            )}
          </div>
          <div className="ml-auto flex items-center gap-2">
            <label className="text-xs text-text-2" htmlFor="tools-device-select">
              {t('tools.deviceLabel')}
            </label>
            <select
              id="tools-device-select"
              value={selected}
              onChange={(e) => setSelected(e.target.value)}
              disabled={online.length === 0}
              className={cn(
                'h-8 rounded-md border border-border bg-bg-1 px-2 text-xs text-text-0',
                'focus:outline-none focus:ring-2 focus:ring-accent',
                'disabled:cursor-not-allowed disabled:opacity-50',
              )}
            >
              {online.length === 0 ? (
                <option value="">{t('tools.noDeviceTitle')}</option>
              ) : (
                online.map((d) => (
                  <option key={d.serial} value={d.serial}>
                    {d.model ?? d.product ?? d.serial} · {d.serial}
                  </option>
                ))
              )}
            </select>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void refresh()}
              disabled={loading}
            >
              <RefreshCw className={cn('h-4 w-4', loading && 'animate-spin')} />
              {t('tools.refresh')}
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* Wireless tcpip sits above the device-required cards: it's
          the "stop using USB" first step, so anchoring it visually
          here mirrors the user's mental model. */}
      <WirelessTcpipCard serial={selected} />

      {online.length === 0 ? (
        <Card className="border-warning">
          <CardContent className="flex items-start gap-3 text-sm">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-warning" />
            <div className="space-y-1">
              <div className="font-semibold text-text-0">{t('tools.noDeviceTitle')}</div>
              <div className="text-text-2">{t('tools.noDeviceDesc')}</div>
            </div>
          </CardContent>
        </Card>
      ) : (
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
          {/* `key={selected}` forces each card to remount when the
              user flips devices -- otherwise per-card state (running
              rows, logs, ...) would persist across an unrelated
              device change. */}
          <MiuiUsbInstallCard key={`miui:${selected}`} serial={selected} />
          <ScreenshotCard key={`shot:${selected}`} serial={selected} />
          <DevOptionsCard key={`devop:${selected}`} serial={selected} />
          <FreezeAppsCard key={`freeze:${selected}`} serial={selected} />
          <BugReportCard key={`bug:${selected}`} serial={selected} />
        </div>
      )}
    </div>
  );
}
