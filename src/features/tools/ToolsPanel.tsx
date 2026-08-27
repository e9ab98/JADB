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
import { Section } from './_shared/Section';
import { MiuiUsbInstallCard } from './MiuiUsbInstallCard';
import { ScreenshotCard } from './ScreenshotCard';
import { WirelessTcpipCard } from './WirelessTcpipCard';
import { DevOptionsCard } from './DevOptionsCard';
import { FreezeAppsCard } from './FreezeAppsCard';
import { BugReportCard } from './BugReportCard';

/**
 * Tools tab content. A "catch-all" for one-off helper actions that
 * don't deserve their own route. The page is split into three
 * semantic sections so the user can scan to the right group
 * instead of hunting through a flat grid of unrelated cards:
 *
 *   1. Connection       -- WirelessTcpipCard (full-width)
 *   2. Device Actions   -- MiuiUsbInstall / Screenshot / BugReport /
 *                          DevOptions (3-col grid)
 *   3. App Management   -- FreezeAppsCard (full-width; needs the
 *                          room for its package list)
 *
 * `key={selected}` on every card forces a remount when the user
 * flips the device dropdown, so per-card state (running rows,
 * logs, ...) doesn't leak across unrelated device changes.
 *
 * We pull the device list the same way the Fastboot/Recovery
 * panels do (5 s poll) so a freshly-attached USB device shows up
 * without a manual reload. We deliberately don't reuse the ADB
 * panel's selection state -- tools tend to run against a specific
 * device and the user should be able to flip between serials
 * without a cross-tab reshuffle.
 */
export function ToolsPanel() {
  const { t } = useTranslation();
  const adbPath = useSettingsStore((s) => s.settings?.adbPath ?? null);
  const [devices, setDevices] = useState<AdbDevice[]>([]);
  const [loading, setLoading] = useState(false);
  // The selected serial. Defaults to the first online device; the
  // user can override via the dropdown.
  const [selected, setSelected] = useState<string>('');

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

  const noDevice = online.length === 0;

  return (
    <div className="space-y-4">
      {/* Top header: tools-page title + online-device count +
          device dropdown + manual refresh. Kept compact so the
          sections below get the visible real estate. */}
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
              disabled={noDevice}
              className={cn(
                'h-8 rounded-md border border-border bg-bg-1 px-2 text-xs text-text-0',
                'focus:outline-none focus:ring-2 focus:ring-accent',
                'disabled:cursor-not-allowed disabled:opacity-50',
              )}
            >
              {noDevice ? (
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

      {/* Section 1: Connection. The wireless tcpip card lives here
          because it's the "switch from USB to Wi-Fi" first step
          and the rest of the tools assume a device is reachable.
          Rendered even when no device is online so the user can
          pair a fresh one without flipping tabs. */}
      <Section
        title={t('tools.section.connection.title')}
        subtitle={t('tools.section.connection.subtitle')}
      >
        <WirelessTcpipCard serial={selected} />
      </Section>

      {/* Sections 2 + 3 only render when a device is online --
          every card under them needs an adb-attached serial. */}
      {noDevice ? (
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
        <>
          {/* Section 2: Device Actions. Three single-action cards
              in the first row (sized to `xl` so all three fit on a
              laptop display without scrolling), then the wider
              DevOptions card alone in the second row so it can
              keep its 2-column switch grid without being
              crammed. */}
          <Section
            title={t('tools.section.device.title')}
            subtitle={t('tools.section.device.subtitle')}
          >
            <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
              <MiuiUsbInstallCard key={`miui:${selected}`} serial={selected} />
              <ScreenshotCard key={`shot:${selected}`} serial={selected} />
              <BugReportCard key={`bug:${selected}`} serial={selected} />
            </div>
            <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
              <DevOptionsCard key={`devop:${selected}`} serial={selected} />
            </div>
          </Section>

          {/* Section 3: App Management. FreezeAppsCard gets full
              width because its package list needs ~600 px to
              render cleanly with the badge column + checkbox +
              text without wrapping the package name awkwardly. */}
          <Section
            title={t('tools.section.app.title')}
            subtitle={t('tools.section.app.subtitle')}
          >
            <FreezeAppsCard key={`freeze:${selected}`} serial={selected} />
          </Section>
        </>
      )}
    </div>
  );
}
