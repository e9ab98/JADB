import { NavLink } from 'react-router-dom';
import {
  ShieldCheck,
  Settings as SettingsIcon,
  Usb,
  Sparkles,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import { toast } from 'sonner';
import { checkForUpdate } from '@/lib/update';
import { UpdateToast } from '@/features/updateNotification/UpdateToast';
import { ExternalLink } from 'lucide-react';
import { cn } from '@/lib/utils';

type NavItem = {
  to: string;
  labelKey: string;
  icon: typeof Usb;
  end?: boolean;
};

const NAV_ITEMS: NavItem[] = [
  { to: '/adb', labelKey: 'nav.adb', icon: Usb, end: false },
  { to: '/apps-tools', labelKey: 'nav.apps', icon: Sparkles, end: false },
  { to: '/sign', labelKey: 'nav.sign', icon: ShieldCheck, end: false },
  { to: '/settings', labelKey: 'nav.settings', icon: SettingsIcon, end: false },
];

const UPDATE_TOAST_ID = 'jadb-update';

function SidebarFooter() {
  const { t } = useTranslation();
  return (
    <div className="border-t border-border p-3 text-xs text-text-2">
      <div className="flex items-center justify-between">
        <span>{t('sidebar.version', { version: 'v0.1.0' })}</span>
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
  return (
    <aside className="flex h-full w-56 shrink-0 flex-col border-r border-border bg-bg-0">
      <div className="flex h-14 items-center gap-2 border-b border-border px-4">
        <div className="grid h-7 w-7 place-items-center rounded-md bg-brand text-sm font-bold text-white">J</div>
        <span className="font-semibold text-text-0">{t('common.appName')}</span>
        <span className="ml-auto text-xs text-text-2">v0.1.0</span>
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
