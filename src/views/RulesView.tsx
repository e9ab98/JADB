import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import { toast } from 'sonner';
import {
  AlertTriangle,
  Box,
  CheckCircle2,
  ExternalLink,
  Filter,
  FolderOpen,
  Loader2,
  RefreshCw,
  Search,
  ShieldCheck,
} from 'lucide-react';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';
import {
  analyzeWithRules,
  listRules,
  type MatchedRule,
  type RuleReport,
  type RuleSet,
} from '@/ipc/rules';

type Severity = 'all' | 'info' | 'warn' | 'danger';

/**
 * Standalone rule-analysis console. Lives at `/rules` and is reachable
 * from the sidebar. The Analyze view already auto-runs the bundled
 * rule packs when a fresh APK is picked, but power users want:
 *
 *   1. To pick a *specific* subset of rule packs (skip bundled
 *      starter, only run LibChecker native-libs, etc.).
 *   2. To see rich LibChecker metadata (label / dev_team /
 *      zh_description / source_link) inline — the Analyze view keeps
 *      that behind a hover popover because space is tight. The
 *      dedicated view is where you go to actually *read* the hits.
 *
 * Nothing in here changes the backend contract — the same
 * `analyzeWithRules` IPC powers both the Analyze dashboard and this
 * page. We just render the result differently.
 */
export function RulesView() {
  const { t } = useTranslation();

  const [apkPath, setApkPath] = useState<string | null>(null);
  const [packs, setPacks] = useState<RuleSet[] | null>(null);
  // Set of selected rule-set ids. `null` means "no rule packs
  // available yet", not "deselected everything".
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [report, setReport] = useState<RuleReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [severity, setSeverity] = useState<Severity>('all');
  const [query, setQuery] = useState('');

  // Re-fetch the installed rule packs whenever the user lands on this
  // view; the user might have just installed LibChecker rules from
  // Settings → Tools and expects them to show up here.
  useEffect(() => {
    let cancelled = false;
    listRules()
      .then((sets) => {
        if (cancelled) return;
        setPacks(sets);
        // Default-select every installed pack. Keeping the
        // selection sticky across renders lets the user toggle a
        // few off and rerun without us silently re-selecting
        // everything.
        setSelected((prev) => {
          if (prev.size > 0) {
            const allowed = new Set(sets.map((s) => s.id));
            const next = new Set<string>();
            for (const id of prev) {
              if (allowed.has(id)) next.add(id);
            }
            return next.size > 0 ? next : new Set(sets.map((s) => s.id));
          }
          return new Set(sets.map((s) => s.id));
        });
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        setPacks([]);
        setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function pickApk() {
    try {
      const p = await openDialog({
        multiple: false,
        filters: [{ name: 'APK', extensions: ['apk'] }],
      });
      if (typeof p === 'string') {
        setApkPath(p);
        setReport(null);
        setError(null);
      }
    } catch (e) {
      toast.error(String(e));
    }
  }

  function togglePack(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function selectAll() {
    if (!packs) return;
    setSelected(new Set(packs.map((s) => s.id)));
  }

  function selectNone() {
    setSelected(new Set());
  }

  async function run() {
    if (!apkPath) return;
    if (selected.size === 0) {
      toast.error(t('rules.noPackSelected'));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const r = await analyzeWithRules(apkPath, Array.from(selected));
      setReport(r);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
      toast.error(msg);
    } finally {
      setBusy(false);
    }
  }

  async function openLink(url: string) {
    try {
      // openUrl from tauri-plugin-opener already enforces http(s)
      // for us; the extra guard in RuleReportList.tsx applies there
      // too. We re-validate here as belt + suspenders because the
      // `rules.sourceLink` field can come from any installed rule
      // pack, including third-party ones.
      if (!/^https?:\/\//i.test(url.trim())) {
        toast.error(t('rules.invalidSourceLink'));
        return;
      }
      await openUrl(url);
    } catch (e) {
      toast.error(String(e));
    }
  }

  return (
    <div className="space-y-6 p-6">
      <header className="space-y-1">
        <h1 className="text-2xl font-semibold text-text-0">{t('rules.title')}</h1>
        <p className="text-sm text-text-2">{t('rules.subtitle')}</p>
      </header>

      {/* APK + ruleset picker */}
      <Card>
        <CardContent className="space-y-4">
          <div className="flex flex-wrap items-center gap-2">
            <Button onClick={pickApk} variant="outline" disabled={busy}>
              <FolderOpen className="h-4 w-4" />
              {t('rules.pickApk')}
            </Button>
            {apkPath && (
              <span
                className="max-w-[480px] truncate rounded-md border border-border bg-bg-2 px-2 py-1 font-mono text-xs text-text-1"
                title={apkPath}
              >
                {apkPath}
              </span>
            )}
            <Button onClick={run} disabled={busy || !apkPath || selected.size === 0}>
              {busy ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <ShieldCheck className="h-4 w-4" />
              )}
              {busy ? t('rules.running') : t('rules.run')}
            </Button>
            {report && (
              <Button variant="ghost" onClick={run} disabled={busy}>
                <RefreshCw className="h-4 w-4" />
                {t('rules.rerun')}
              </Button>
            )}
          </div>

          {packs === null ? (
            <div className="flex items-center gap-2 text-sm text-text-2">
              <Loader2 className="h-4 w-4 animate-spin" />
              {t('rules.loadingPacks')}
            </div>
          ) : packs.length === 0 ? (
            <div className="rounded-md border border-warning/40 bg-warning/10 px-3 py-2 text-xs text-text-1">
              <div className="flex items-start gap-2">
                <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-warning" />
                <span>{t('rules.noPacksHint')}</span>
              </div>
            </div>
          ) : (
            <div className="space-y-2">
              <div className="flex items-center gap-2 text-xs text-text-2">
                <span>{t('rules.availablePacks', { count: packs.length })}</span>
                <span className="text-text-2">·</span>
                <button
                  type="button"
                  className="text-brand hover:underline"
                  onClick={selectAll}
                >
                  {t('rules.selectAll')}
                </button>
                <span className="text-text-2">·</span>
                <button
                  type="button"
                  className="text-brand hover:underline"
                  onClick={selectNone}
                >
                  {t('rules.selectNone')}
                </button>
              </div>
              <ul className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
                {packs.map((pack) => {
                  const checked = selected.has(pack.id);
                  return (
                    <li key={pack.id}>
                      <label
                        className={cn(
                          'flex cursor-pointer items-start gap-2 rounded-lg border px-3 py-2 text-sm transition-colors',
                          checked
                            ? 'border-brand bg-brand/5'
                            : 'border-border bg-bg-1 hover:border-text-2',
                        )}
                      >
                        <input
                          type="checkbox"
                          className="mt-1 h-3.5 w-3.5 shrink-0 accent-brand"
                          checked={checked}
                          onChange={() => togglePack(pack.id)}
                        />
                        <span className="min-w-0 flex-1">
                          <span className="block truncate font-medium text-text-0">
                            {pack.name}
                          </span>
                          {pack.version && (
                            <span className="block text-[11px] text-text-2">
                              v{pack.version}
                            </span>
                          )}
                          {pack.description && (
                            <span className="mt-1 block text-xs text-text-2">
                              {pack.description}
                            </span>
                          )}
                        </span>
                      </label>
                    </li>
                  );
                })}
              </ul>
            </div>
          )}
        </CardContent>
      </Card>

      {error && (
        <Card className="border-danger/60">
          <CardContent className="flex items-start gap-3 text-sm">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-danger" />
            <span className="break-all font-mono text-xs text-text-1">{error}</span>
          </CardContent>
        </Card>
      )}

      {report && (
        <RulesResults
          report={report}
          severity={severity}
          onSeverityChange={setSeverity}
          query={query}
          onQueryChange={setQuery}
          onOpenLink={openLink}
        />
      )}
    </div>
  );
}

/**
 * Filterable, scrollable list of matched components grouped by
 * category. The metadata for each hit is rendered inline (label +
 * dev_team + zh_description + source link) rather than behind a
 * hover, because this view exists precisely so users can read the
 * hits without having to aim at a tiny hover popover.
 */
function RulesResults({
  report,
  severity,
  onSeverityChange,
  query,
  onQueryChange,
  onOpenLink,
}: {
  report: RuleReport;
  severity: Severity;
  onSeverityChange: (s: Severity) => void;
  query: string;
  onQueryChange: (q: string) => void;
  onOpenLink: (url: string) => void;
}) {
  const { t } = useTranslation();
  const trimmedQuery = query.trim().toLowerCase();

  // Build a flat list of (category, hit) rows that survive both
  // filters. Filtering happens client-side after `analyzeWithRules`
  // returns — the backend already filters to installed packs and we
  // do not want to pay another IPC roundtrip for a UI filter.
  const rows = useMemo(() => {
    type Row = {
      category: keyof RuleReport['components'];
      categoryKey: string;
      hit: RuleReport['components'][keyof RuleReport['components']][number];
    };
    const all: Row[] = [];
    const cats: Array<{
      key: keyof RuleReport['components'];
      categoryKey: string;
    }> = [
      { key: 'native_libraries', categoryKey: 'rules.catNativeLibs' },
      { key: 'activities', categoryKey: 'rules.catActivities' },
      { key: 'services', categoryKey: 'rules.catServices' },
      { key: 'receivers', categoryKey: 'rules.catReceivers' },
      { key: 'providers', categoryKey: 'rules.catProviders' },
    ];
    for (const c of cats) {
      for (const hit of report.components[c.key]) {
        if (!hit.matched_rule) continue;
        if (severity !== 'all' && hit.matched_rule.severity?.toLowerCase() !== severity) {
          continue;
        }
        if (trimmedQuery) {
          const hay = [
            hit.name,
            hit.matched_rule.metadata?.label ?? '',
            hit.matched_rule.metadata?.dev_team ?? '',
          ]
            .join('\n')
            .toLowerCase();
          if (!hay.includes(trimmedQuery)) continue;
        }
        all.push({ category: c.key, categoryKey: c.categoryKey, hit });
      }
    }
    return all;
  }, [report, severity, trimmedQuery]);

  const totalsBySeverity = useMemo(() => {
    let info = 0;
    let warn = 0;
    let danger = 0;
    for (const cat of Object.values(report.components)) {
      for (const h of cat) {
        if (!h.matched_rule) continue;
        const sev = h.matched_rule.severity?.toLowerCase();
        if (sev === 'danger') danger += 1;
        else if (sev === 'warn' || sev === 'warning') warn += 1;
        else info += 1;
      }
    }
    return { info, warn, danger };
  }, [report]);

  // Group filtered rows by category so we can render them in the
  // same order the Analyze view uses (native libs first, then the
  // four component types).
  const grouped = useMemo(() => {
    const cats: Array<keyof RuleReport['components']> = [
      'native_libraries',
      'activities',
      'services',
      'receivers',
      'providers',
    ];
    const map = new Map<keyof RuleReport['components'], typeof rows>();
    for (const key of cats) map.set(key, []);
    for (const row of rows) map.get(row.category)!.push(row);
    return cats.map((key) => ({
      key,
      categoryKey: `rules.cat${
        key === 'native_libraries'
          ? 'NativeLibs'
          : key.charAt(0).toUpperCase() + key.slice(1)
      }`,
      rows: map.get(key) ?? [],
    }));
  }, [rows]);

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-2">
        <div className="flex items-center gap-2 text-xs text-text-2">
          <CheckCircle2 className="h-4 w-4 text-success" />
          {t('rules.summaryMatched', {
            matched: report.total_matched,
            total: rows.length,
          })}
        </div>
        <Badge variant="secondary">{t('rules.sevInfo', { count: totalsBySeverity.info })}</Badge>
        <Badge variant="warning">{t('rules.sevWarn', { count: totalsBySeverity.warn })}</Badge>
        <Badge variant="danger">{t('rules.sevDanger', { count: totalsBySeverity.danger })}</Badge>
      </div>

      <div className="flex flex-wrap items-center gap-3">
        <div className="flex items-center gap-2">
          <Filter className="h-4 w-4 text-text-2" />
          <span className="text-xs text-text-2">{t('rules.filterSeverity')}</span>
          <div className="inline-flex rounded-md border border-border bg-bg-1 p-0.5">
            {(['all', 'info', 'warn', 'danger'] as const).map((s) => (
              <button
                key={s}
                type="button"
                onClick={() => onSeverityChange(s)}
                className={cn(
                  'rounded px-2 py-0.5 text-xs transition-colors',
                  severity === s
                    ? 'bg-brand text-white'
                    : 'text-text-1 hover:bg-bg-2',
                )}
              >
                {t(`rules.sevTab.${s}`)}
              </button>
            ))}
          </div>
        </div>
        <div className="relative ml-auto min-w-[200px] flex-1 max-w-md">
          <Search className="absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-text-2" />
          <Input
            value={query}
            onChange={(e) => onQueryChange(e.target.value)}
            placeholder={t('rules.searchPlaceholder')}
            className="pl-7"
          />
        </div>
      </div>

      {rows.length === 0 ? (
        <Card>
          <CardContent className="text-sm text-text-2">
            {t('rules.noMatches')}
          </CardContent>
        </Card>
      ) : (
        <div className="grid gap-4 md:grid-cols-2">
          {grouped.map(({ key, categoryKey, rows: catRows }) => (
            <CategoryCard
              key={key}
              categoryKey={categoryKey}
              rows={catRows}
              onOpenLink={onOpenLink}
            />
          ))}
        </div>
      )}
    </div>
  );
}

const SEVERITY_VARIANT: Record<string, 'danger' | 'warning' | 'secondary'> = {
  danger: 'danger',
  warn: 'warning',
  warning: 'warning',
  info: 'secondary',
};

function CategoryCard({
  categoryKey,
  rows,
  onOpenLink,
}: {
  categoryKey: string;
  rows: Array<{
    category: keyof RuleReport['components'];
    categoryKey: string;
    hit: RuleReport['components'][keyof RuleReport['components']][number];
  }>;
  onOpenLink: (url: string) => void;
}) {
  const { t } = useTranslation();
  if (rows.length === 0) {
    // Hide empty categories entirely. The summary badges at the top
    // already give the user an at-a-glance count; rendering five
    // empty cards just clutters the page when only one or two
    // categories have matches.
    return null;
  }
  return (
    <Card>
      <CardContent>
        <div className="mb-3 flex items-center gap-2">
          <Box className="h-4 w-4 text-brand" />
          <span className="text-sm font-semibold text-text-0">
            {t(categoryKey)}
          </span>
          <span className="ml-auto text-xs text-text-2">
            {t('rules.matchedCount', { count: rows.length })}
          </span>
        </div>
        <ul className="space-y-3">
          {rows.map((row) => (
            <HitCard key={row.hit.name} hit={row.hit} onOpenLink={onOpenLink} />
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}

function safeSourceLink(raw: string | null | undefined): string | null {
  if (!raw) return null;
  const trimmed = raw.trim();
  if (!/^https?:\/\//i.test(trimmed)) return null;
  return trimmed;
}

function HitCard({
  hit,
  onOpenLink,
}: {
  hit: RuleReport['components'][keyof RuleReport['components']][number];
  onOpenLink: (url: string) => void;
}) {
  const { t } = useTranslation();
  const m: MatchedRule | undefined | null = hit.matched_rule;
  const sev = (m?.severity ?? 'info').toLowerCase();
  const variant = SEVERITY_VARIANT[sev] ?? 'secondary';
  const meta = m?.metadata;
  const link = safeSourceLink(meta?.source_link);
  return (
    <li className="rounded-lg border border-border bg-bg-1 p-3">
      <div className="flex items-baseline gap-2">
        <span
          className="min-w-0 flex-1 truncate font-mono text-xs text-text-0"
          title={hit.name}
        >
          {hit.name}
        </span>
        <Badge
          variant={variant}
          className="shrink-0 px-1.5 py-0 text-[10px] font-bold uppercase tracking-wider"
        >
          {m?.severity ?? '—'}
        </Badge>
      </div>
      {meta?.label?.trim() && (
        <div className="mt-1.5 text-sm font-semibold text-text-0">
          {meta.label.trim()}
        </div>
      )}
      {meta?.dev_team?.trim() && (
        <div className="text-[11px] text-text-2">{meta.dev_team.trim()}</div>
      )}
      {meta?.zh_description?.trim() && (
        <p className="mt-1.5 max-h-40 overflow-auto whitespace-pre-wrap break-words text-xs text-text-1">
          {meta.zh_description}
        </p>
      )}
      {link && (
        <button
          type="button"
          onClick={() => onOpenLink(link)}
          className="mt-2 inline-flex items-center gap-1 rounded-md border border-border bg-bg-2 px-2 py-0.5 text-[11px] text-text-1 hover:border-accent hover:text-accent"
        >
          <ExternalLink className="h-3 w-3" />
          <span className="truncate">{link}</span>
        </button>
      )}
      {!meta?.label && !meta?.dev_team && !meta?.zh_description && !link && (
        <p className="mt-1 text-[11px] text-text-2">{t('rules.noMetadata')}</p>
      )}
    </li>
  );
}
