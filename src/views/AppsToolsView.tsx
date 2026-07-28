import { useTranslation } from 'react-i18next';
import '@/i18n';
import { Code2, FolderOpen, PackageOpen, Search, ArrowUpRight } from 'lucide-react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { cn } from '@/lib/utils';
import { launchJadxGui } from '@/ipc/jadx';
import {
  openAnalyzeWindow,
  openDecompileWindow,
  openRepackageWindow,
} from '@/ipc/window';
import { toast } from 'sonner';

type ToolCard = {
  id: 'jadx' | 'decompile' | 'repackage' | 'analyze';
  titleKey: string;
  descKey: string;
  icon: typeof Code2;
  accent: string;
  action: () => void;
};

/**
 * Sub-menu for the sidebar "应用" tab. The four APK tools are rendered
 * as cards so the user can see them at a glance — JADX launches the
 * standalone JADX-GUI in a detached process; 分析 / 反编译 / 重打包 open
 * their own OS-level windows via the `open_*_window` Rust commands so
 * the main shell stays interactive.
 */
export function AppsToolsView() {
  const { t } = useTranslation();

  const cards: ToolCard[] = [
    {
      id: 'analyze',
      titleKey: 'appsTools.analyzeTitle',
      descKey: 'appsTools.analyzeDesc',
      icon: Search,
      accent: 'from-sky-500/15 to-sky-500/0 text-sky-400',
      action: () => {
        openAnalyzeWindow().catch((e) => toast.error(String(e)));
      },
    },
    {
      id: 'jadx',
      titleKey: 'appsTools.jadxTitle',
      descKey: 'appsTools.jadxDesc',
      icon: Code2,
      accent: 'from-indigo-500/15 to-indigo-500/0 text-indigo-400',
      action: () => {
        launchJadxGui().catch((e) => toast.error(String(e)));
      },
    },
    {
      id: 'decompile',
      titleKey: 'appsTools.decompileTitle',
      descKey: 'appsTools.decompileDesc',
      icon: FolderOpen,
      accent: 'from-emerald-500/15 to-emerald-500/0 text-emerald-400',
      action: () => {
        openDecompileWindow().catch((e) => toast.error(String(e)));
      },
    },
    {
      id: 'repackage',
      titleKey: 'appsTools.repackageTitle',
      descKey: 'appsTools.repackageDesc',
      icon: PackageOpen,
      accent: 'from-amber-500/15 to-amber-500/0 text-amber-400',
      action: () => {
        openRepackageWindow().catch((e) => toast.error(String(e)));
      },
    },
  ];

  return (
    <div className="space-y-6 p-8">
      <div className="space-y-1">
        <h1 className="text-2xl font-semibold text-text-0">{t('appsTools.title')}</h1>
        <p className="text-sm text-text-2">{t('appsTools.subtitle')}</p>
      </div>

      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
        {cards.map(({ id, titleKey, descKey, icon: Icon, accent, action }) => (
          <Card
            key={id}
            role="button"
            tabIndex={0}
            onClick={action}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                action();
              }
            }}
            className={cn(
              'group relative cursor-pointer overflow-hidden transition-all',
              'hover:-translate-y-0.5 hover:border-brand hover:shadow-card',
              'focus:outline-none focus-visible:ring-2 focus-visible:ring-brand',
            )}
          >
            <div
              className={cn(
                'pointer-events-none absolute inset-0 bg-gradient-to-br opacity-60 transition-opacity group-hover:opacity-100',
                accent,
              )}
              aria-hidden
            />
            <CardHeader className="relative">
              <div className="flex items-start justify-between">
                <div
                  className={cn(
                    'grid h-11 w-11 place-items-center rounded-xl bg-bg-2 ring-1 ring-border',
                  )}
                >
                  <Icon className="h-5 w-5" />
                </div>
                <ArrowUpRight className="h-4 w-4 text-text-2 transition-transform group-hover:-translate-y-0.5 group-hover:translate-x-0.5 group-hover:text-text-0" />
              </div>
              <CardTitle className="pt-2">{t(titleKey)}</CardTitle>
              <CardDescription>{t(descKey)}</CardDescription>
            </CardHeader>
            <CardContent className="relative">
              <div className="text-xs text-text-2">{t('appsTools.openHint')}</div>
            </CardContent>
          </Card>
        ))}
      </div>
    </div>
  );
}
