import { useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { AdbConnectionPanel } from '@/features/adb/AdbConnectionPanel';
import { FastbootPanel } from '@/features/fastboot/FastbootPanel';
import { RecoveryPanel } from '@/features/recovery/RecoveryPanel';

/** Whitelisted tab ids accepted by `?tab=`. Anything else falls back
 *  to `adb` so the URL never strands the user on a missing tab. */
type DeviceTab = 'adb' | 'recovery' | 'fastboot';
const DEFAULT_TAB: DeviceTab = 'adb';

function isDeviceTab(value: string | null): value is DeviceTab {
  return value === 'adb' || value === 'recovery' || value === 'fastboot';
}

/**
 * Top-level "设备" view. Splits adb-protocol and fastboot-protocol
 * concerns into two tabs so the sidebar entry stays a single concept
 * ("device") instead of leaking implementation protocols into the
 * navigation. The active tab is reflected in `?tab=` so direct links
 * like `/device?tab=fastboot` open straight to the right panel.
 */
export function DeviceView() {
  const { t } = useTranslation();
  const [searchParams, setSearchParams] = useSearchParams();
  const raw = searchParams.get('tab');
  const tab: DeviceTab = isDeviceTab(raw) ? raw : DEFAULT_TAB;

  function setTab(next: DeviceTab) {
    const sp = new URLSearchParams(searchParams);
    sp.set('tab', next);
    setSearchParams(sp, { replace: true });
  }

  return (
    <div className="space-y-4 p-6">
      <h1 className="text-2xl font-semibold text-text-0">
        {t('nav.device')}
      </h1>
      <Tabs value={tab} onValueChange={(v) => isDeviceTab(v) && setTab(v)}>
        <TabsList>
          <TabsTrigger value="adb">{t('recovery.deviceTab')}</TabsTrigger>
          <TabsTrigger value="recovery">{t('recovery.recoveryTab')}</TabsTrigger>
          <TabsTrigger value="fastboot">{t('recovery.fastbootTab')}</TabsTrigger>
        </TabsList>
        <TabsContent value="adb">
          <AdbConnectionPanel />
        </TabsContent>
        <TabsContent value="recovery">
          <RecoveryPanel />
        </TabsContent>
        <TabsContent value="fastboot">
          <FastbootPanel />
        </TabsContent>
      </Tabs>
    </div>
  );
}
