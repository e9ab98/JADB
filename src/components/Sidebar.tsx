import { NavLink } from 'react-router-dom';
import {
  ShieldCheck,
  Settings as SettingsIcon,
  Usb,
  Sparkles,
  FileSearch,
  GitCompare,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import { toast } from 'sonner';
import { checkForUpdate } from '@/lib/update';
import { UpdateToast } from '@/features/updateNotification/UpdateToast';
import { ExternalLink } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Crown } from 'lucide-react';
import { useLicenseStore } from '@/store/license';
import { useAppVersion } from '@/hooks/useAppVersion';

type NavItem = {
  to: string;
  labelKey: string;
  icon: typeof Usb;
  end?: boolean;
};

const NAV_ITEMS: NavItem[] = [
  { to: '/adb', labelKey: 'nav.adb', icon: Usb, end: false },
  { to: '/apps-tools', labelKey: 'nav.apps', icon: Sparkles, end: false },
  { to: '/rules', labelKey: 'nav.rules', icon: FileSearch, end: false },
  { to: '/compare', labelKey: 'nav.compare', icon: GitCompare, end: false },
  { to: '/sign', labelKey: 'nav.sign', icon: ShieldCheck, end: false },
  { to: '/settings', labelKey: 'nav.settings', icon: SettingsIcon, end: false },
];

const UPDATE_TOAST_ID = 'jadb-update';

function SidebarFooter() {
  const { t } = useTranslation();
  const status = useLicenseStore((s) => s.status);
  const active = status?.state === 'active';
  const versionInfo = useAppVersion();
  // Display the version string the user actually sees next to the
  // check-update button. Falls back to an em-dash so the layout
  // doesn't shift if the Rust command is still warming up.
  const versionLabel = versionInfo ? `v${versionInfo.version}` : 'v—';
  const isDebug = versionInfo?.profile === 'debug';
  return (
    <div className="border-t border-border p-3 text-xs text-text-2">
      <NavLink to="/settings?tab=license" className="mb-2 flex items-center gap-2 rounded-md border border-border bg-bg-1 px-3 py-2 hover:border-brand">
        <Crown className={cn("h-4 w-4", active ? "text-warning" : "text-text-2")} />
        <span className="font-medium text-text-0">{active ? t('license.vip') : t('license.free')}</span>
        <span className="ml-auto text-brand">{active ? t('license.manage') : t('license.activate')}</span>
      </NavLink>
      <div className="flex items-center justify-between">
        <span className="inline-flex items-center gap-1">
          {t('sidebar.version', { version: versionLabel })}
          {isDebug && (
            <span
              className="rounded bg-amber-500/15 px-1 text-[10px] font-bold text-amber-600"
              title={t('sidebar.devBuild')}
            >
              DEV
            </span>
          )}
        </span>
        <button
          type="button"
          onClick={() => {
            checkForUpdate({ timeout: 5000 }, true).then((update) => {
              if (update) {
                toast(<UpdateToast version={update.version} notes={update.notes} />, {
                  id: UPDATE_TOAST_ID,
                  duration: Infinity,
                });
              } else {
                toast.success(t('sidebar.upToDate'));
              }
            }).catch(() => toast.error(t('sidebar.updateFailed')));
          }}
          className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-text-2 transition-colors hover:bg-bg-2 hover:text-text-0"
          title={t('sidebar.checkUpdates')}
        >
          <ExternalLink className="h-3 w-3" />
          {t('sidebar.checkUpdates')}
        </button>
      </div>
    </div>
  );
}

export function Sidebar() {
  const { t } = useTranslation();
  const versionInfo = useAppVersion();
  // Header chip mirrors the footer version so the user can see
  // the build number without scrolling. We deliberately use the
  // same `useAppVersion` hook — the IPC round-trip is amortised
  // by React's request-dedup logic since the hook runs once in
  // each component instance but the underlying `invoke` resolves
  // on the same Tauri command.
  const headerVersion = versionInfo ? `v${versionInfo.version}` : 'v—';
  return (
    <aside className="flex h-full w-56 shrink-0 flex-col border-r border-border bg-bg-0">
      <div className="flex h-14 items-center gap-2 border-b border-border px-4">
        <div className="grid h-7 w-7 place-items-center rounded-md bg-brand text-sm font-bold text-white">J</div>
        <span className="font-semibold text-text-0">{t('common.appName')}</span>
        <span className="ml-auto text-xs text-text-2">{headerVersion}</span>
      </div>
      <nav className="flex-1 space-y-0.5 p-2 overflow-y-auto">
        {NAV_ITEMS.map(({ to, labelKey, icon: Icon, end }) => (
          <NavLink
            key={to}
            to={to}
            end={end ?? false}
            className={({ isActive }) =>
              cn(
                'flex items-center gap-2 rounded-md px-3 py-2 text-sm transition-colors',
                isActive ? 'bg-brand-soft text-brand-strong' : 'text-text-1 hover:bg-bg-2',
              )
            }
          >
            <Icon className="h-4 w-4" />
            {t(labelKey)}
          </NavLink>
        ))}
      </nav>
      <SidebarFooter />
    </aside>
  );
}
