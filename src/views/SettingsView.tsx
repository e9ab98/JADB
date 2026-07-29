import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useSearchParams } from 'react-router-dom';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { GeneralTab } from '@/features/settings/GeneralTab';
import { ToolsTab } from '@/features/settings/ToolsTab';
import { SignaturesTab } from '@/features/settings/SignaturesTab';
import { useSettingsStore } from '@/store/settings';
import { LicenseTab } from '@/features/settings/LicenseTab';

const TAB_VALUES = ['general', 'tools', 'signatures', 'license'] as const;
type TabValue = (typeof TAB_VALUES)[number];

function isTabValue(v: string | null): v is TabValue {
  return v !== null && (TAB_VALUES as readonly string[]).includes(v);
}

export function SettingsView() {
  const { t } = useTranslation();
  const refresh = useSettingsStore((s) => s.refresh);
  const [searchParams, setSearchParams] = useSearchParams();

  // The SettingsView is reachable via deep links from other tabs (e.g. the
  // ADB panel sending users here to install adb). We honour a `?tab=`
  // query string so those links can land on the right tab directly.
  const requested = searchParams.get('tab');
  const value: TabValue = isTabValue(requested) ? requested : 'general';

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return (
    <div className="space-y-6 p-8">
      <h1 className="text-2xl font-semibold text-text-0">{t('settings.title')}</h1>
      <Tabs
        value={value}
        onValueChange={(next) => {
          if (!isTabValue(next)) return;
          // Keep the URL in sync so reload preserves the active tab.
          const params = new URLSearchParams(searchParams);
          params.set('tab', next);
          setSearchParams(params, { replace: true });
        }}
      >
        <TabsList>
          <TabsTrigger value="general">{t('settings.general')}</TabsTrigger>
          <TabsTrigger value="tools">{t('settings.tools')}</TabsTrigger>
          <TabsTrigger value="signatures">
            {t('settings.signatures')}
          </TabsTrigger>
          <TabsTrigger value="license">{t('license.title')}</TabsTrigger>
        </TabsList>
        <TabsContent value="general">
          <GeneralTab />
        </TabsContent>
        <TabsContent value="tools">
          <ToolsTab />
        </TabsContent>
        <TabsContent value="signatures">
          <SignaturesTab />
        </TabsContent>
        <TabsContent value="license"><LicenseTab /></TabsContent>
      </Tabs>
    </div>
  );
}
