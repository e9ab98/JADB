import { useTranslation } from 'react-i18next';
import { ExternalLink, Info, Box, Activity, Server, Inbox, Database } from 'lucide-react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { toast } from 'sonner';
import { Card, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';
import type { ComponentHit, ComponentMatches, MatchedRule, RuleReport } from '@/ipc/rules';

type Props = { report: RuleReport | null };

/** Restrict upstream links to http(s) so a malicious rule JSON can't
 *  hand us a `file://` or `javascript:` payload. */
function safeSourceLink(raw: string | null | undefined): string | null {
  if (!raw) return null;
  const trimmed = raw.trim();
  if (!/^https?:\/\//i.test(trimmed)) return null;
  return trimmed;
}

function hasMeaningfulMeta(meta: MatchedRule['metadata']): boolean {
  if (!meta) return false;
  return Boolean(
    meta.label?.trim() ||
      meta.dev_team?.trim() ||
      meta.source_link ||
      meta.zh_description?.trim(),
  );
}

const SEVERITY_CLASS: Record<string, string> = {
  danger: 'bg-danger text-white',
  warn: 'bg-warning text-bg-0',
  info: 'bg-info text-white',
};

const SEVERITY_VARIANT: Record<string, 'danger' | 'warning' | 'secondary'> = {
  danger: 'danger',
  warn: 'warning',
  warning: 'warning',
  info: 'secondary',
};

/**
 * Hover popover rendered on rows whose libchecker match has rich
 * metadata (label / dev_team / source_link / zh_description). Same
 * pointer-events trick as the previous iteration — the popover stays
 * open while the user mouses between the row and itself.
 */
function MetaPopover({
  meta,
  onOpenLink,
}: {
  meta: NonNullable<MatchedRule['metadata']>;
  onOpenLink: (url: string) => void;
}) {
  const link = safeSourceLink(meta.source_link);
  return (
    <div
      role="tooltip"
      className={cn(
        'pointer-events-none absolute left-0 top-full z-20 mt-1.5 w-80 max-w-[90vw]',
        'rounded-xl border border-border bg-bg-1 p-3 text-xs text-text-1 shadow-card',
        'invisible opacity-0 transition-all duration-150',
        'group-hover/item:visible group-hover/item:opacity-100',
        'group-hover/item:pointer-events-auto',
      )}
    >
      {(meta.label?.trim() || meta.dev_team?.trim()) && (
        <div className="mb-1 flex items-baseline gap-2">
          {meta.label?.trim() && (
            <span className="font-semibold text-text-0">{meta.label.trim()}</span>
          )}
          {meta.dev_team?.trim() && (
            <span className="text-text-2">{meta.dev_team.trim()}</span>
          )}
        </div>
      )}
      {meta.zh_description?.trim() && (
        <div className="mb-2 max-h-40 overflow-auto whitespace-pre-wrap break-words text-text-1">
          {meta.zh_description}
        </div>
      )}
      {link && (
        <button
          type="button"
          onClick={() => onOpenLink(link)}
          className="inline-flex items-center gap-1 rounded-md border border-border bg-bg-2 px-2 py-0.5 text-text-1 hover:border-accent hover:text-accent"
        >
          <ExternalLink className="h-3 w-3" />
          <span className="truncate">{link}</span>
        </button>
      )}
    </div>
  );
}

function ComponentRow({
  hit,
  onOpenLink,
}: {
  hit: ComponentHit;
  onOpenLink: (url: string) => void;
}) {
  const { t } = useTranslation();
  const m = hit.matched_rule;
  const variant = m
    ? SEVERITY_VARIANT[m.severity?.toLowerCase()] ?? 'secondary'
    : 'secondary';
  return (
    <li
      key={hit.name}
      className={cn(
        'group/item relative flex items-baseline gap-2 py-1.5 text-sm',
        m ? 'text-text-0' : 'text-text-1',
      )}
    >
      <span
        className={cn(
          'min-w-0 flex-1 truncate font-mono text-xs',
          m ? 'text-text-0' : 'text-text-2',
        )}
        title={hit.name}
      >
        {hit.name}
      </span>
      {m ? (
        <>
          <Badge
            variant={variant}
            className={cn(
              'shrink-0 px-1.5 py-0 text-[10px] font-bold uppercase tracking-wider',
              !SEVERITY_CLASS[m.severity?.toLowerCase()] &&
                'bg-bg-2 text-text-1',
            )}
          >
            {m.severity}
          </Badge>
          {hasMeaningfulMeta(m.metadata) && (
            <Info
              className="h-3 w-3 shrink-0 text-text-2 transition-colors group-hover/item:text-text-0"
              aria-label={t('rules.hoverHint')}
            />
          )}
          {hasMeaningfulMeta(m.metadata) && (
            <MetaPopover meta={m.metadata!} onOpenLink={onOpenLink} />
          )}
        </>
      ) : (
        <span className="shrink-0 text-[10px] text-text-2">
          {t('rules.unmatched')}
        </span>
      )}
    </li>
  );
}

type SectionProps = {
  title: string;
  icon: typeof Box;
  hits: ComponentHit[];
  onOpenLink: (url: string) => void;
};

function Section({ title, icon: Icon, hits, onOpenLink }: SectionProps) {
  const { t } = useTranslation();
  const matched = hits.filter((h) => h.matched_rule).length;
  return (
    <Card>
      <CardContent>
        <div className="mb-2 flex items-center gap-2">
          <Icon className="h-4 w-4 text-brand" />
          <span className="text-sm font-semibold text-text-0">{title}</span>
          <span className="ml-auto text-xs text-text-2">
            {matched} / {hits.length} {t('rules.matched')}
          </span>
        </div>
        {hits.length === 0 ? (
          <div className="text-xs text-text-2">—</div>
        ) : (
          <ul className="divide-y divide-border/60">
            {hits.map((h) => (
              <ComponentRow key={h.name} hit={h} onOpenLink={onOpenLink} />
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}

export function RuleReportList({ report }: Props) {
  const { t } = useTranslation();

  async function openLink(url: string) {
    try {
      await openUrl(url);
    } catch (e) {
      toast.error(String(e));
    }
  }

  if (!report || report.total_matched === 0) {
    return (
      <Card>
        <CardContent className="text-sm text-text-2">
          {t('rules.noResults')}
        </CardContent>
      </Card>
    );
  }

  const c: ComponentMatches = report.components;
  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-3 text-xs text-text-2">
        <span>
          <span className="font-semibold text-text-0">{report.total_matched}</span>{' '}
          {t('rules.matched')}
        </span>
        <span className="text-text-2">·</span>
        <span>
          {t('rules.totalCount', {
            count:
              c.native_libraries.length +
              c.activities.length +
              c.services.length +
              c.receivers.length +
              c.providers.length,
          })}
        </span>
      </div>
      <Section
        title={t('rules.sectionNative')}
        icon={Box}
        hits={c.native_libraries}
        onOpenLink={openLink}
      />
      <Section
        title={t('rules.sectionActivities')}
        icon={Activity}
        hits={c.activities}
        onOpenLink={openLink}
      />
      <Section
        title={t('rules.sectionServices')}
        icon={Server}
        hits={c.services}
        onOpenLink={openLink}
      />
      <Section
        title={t('rules.sectionReceivers')}
        icon={Inbox}
        hits={c.receivers}
        onOpenLink={openLink}
      />
      <Section
        title={t('rules.sectionProviders')}
        icon={Database}
        hits={c.providers}
        onOpenLink={openLink}
      />
    </div>
  );
}
