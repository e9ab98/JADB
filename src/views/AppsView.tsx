import { useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import { Smartphone } from 'lucide-react';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { AdbAppsTab } from '@/features/adb/AdbAppsTab';
import { AdbShellTab } from '@/features/adb/AdbShellTab';

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
      </header>

      <main className="flex-1 overflow-auto p-6">
        <Tabs defaultValue="apps">
          <TabsList>
            <TabsTrigger value="apps">{t('adb.appsTab')}</TabsTrigger>
            <TabsTrigger value="shell">{t('adb.shellTab')}</TabsTrigger>
          </TabsList>
          <TabsContent value="apps">
            <AdbAppsTab key={serial} serial={serial} />
          </TabsContent>
          <TabsContent value="shell">
            <AdbShellTab key={serial} serial={serial} />
          </TabsContent>
        </Tabs>
      </main>
    </div>
  );
}
