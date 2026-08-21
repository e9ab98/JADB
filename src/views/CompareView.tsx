import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { toast } from 'sonner';
import {
  AlertTriangle,
  ArrowLeftRight,
  Boxes,
  Check,
  CheckCircle2,
  Download,
  Eraser,
  FileSearch,
  FolderOpen,
  Info,
  Layers,
  Loader2,
  Minus,
  Package as PackageIcon,
  Plus,
  Server,
  ShieldCheck,
} from 'lucide-react';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { cn, formatBytes } from '@/lib/utils';
import { analyzeApk, type ApkInfo } from '@/ipc/analyze';
import {
  analyzeWithRules,
  type RuleReport,
  type RuleSet,
  listRules,
} from '@/ipc/rules';
import { buildCompareReportHtml } from '@/features/compare/reportTemplate';
import { exportApkReport } from '@/ipc/report';
import { useLicenseStore } from '@/store/license';

type Side = 'A' | 'B';

/** One APK's worth of analysis output: local file info + the
 *  analyzeApk result + the optional rule report. */
type SideData = {
  path: string;
  size: number | null;
  info: ApkInfo | null;
  rules: RuleReport | null;
  error: string | null;
};

const EMPTY_SIDE: SideData = {
  path: '',
  size: null,
  info: null,
  rules: null,
  error: null,
};

type DiffRow<T> = {
  added: T[];
  removed: T[];
  shared: T[];
};

function diffSets<T>(a: Iterable<T>, b: Iterable<T>): DiffRow<T> {
  const aSet = new Set(a);
  const bSet = new Set(b);
  const added: T[] = [];
  const removed: T[] = [];
  const shared: T[] = [];
  for (const item of aSet) {
    if (bSet.has(item)) shared.push(item);
    else removed.push(item);
  }
  for (const item of bSet) {
    if (!aSet.has(item)) added.push(item);
  }
  added.sort();
  removed.sort();
  shared.sort();
  return { added, removed, shared };
}

/**
 * Side-by-side APK comparison. Lives at `/compare` and is reachable
 * from the sidebar.
 *
 * Pipeline per side:
 *   1. User picks an APK.
 *   2. We kick off `analyzeApk` immediately so the file picker
 *      round-trip is not wasted on a "Analyze" button click.
 *   3. If at least one rule pack is installed, we also kick off
 *      `analyzeWithRules` with the user-selected rule packs.
 *   4. Both sides run in parallel; the UI surfaces a per-side
 *      progress / error chip so the user knows what's happening.
 *
 * Once both sides have data the diff sections render in stacked
 * cards (metadata → permissions → components → native libs →
 * rule hits). No VIP gating on the diff itself; we only gate the
 * HTML export.
 */
export function CompareView() {
  const { t } = useTranslation();
  const [a, setA] = useState<SideData>(EMPTY_SIDE);
  const [b, setB] = useState<SideData>(EMPTY_SIDE);
  const [packs, setPacks] = useState<RuleSet[] | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());

  // Refresh installed rule packs on mount. Power users will have just
  // installed LibChecker rules from Settings → Tools and expect them
  // to be selectable here.
  useEffect(() => {
    let cancelled = false;
    listRules()
      .then((sets) => {
        if (cancelled) return;
        setPacks(sets);
        setSelected((prev) => {
          if (prev.size > 0) {
            const allowed = new Set(sets.map((s) => s.id));
            const next = new Set<string>();
            for (const id of prev) if (allowed.has(id)) next.add(id);
            return next.size > 0 ? next : new Set(sets.map((s) => s.id));
          }
          return new Set(sets.map((s) => s.id));
        });
      })
      .catch(() => {
        if (!cancelled) setPacks([]);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Single helper that runs the per-side pipeline. Caller swaps the
  // side label (`A` / `B`) for chips / toasts.
  const runForSide = useCallback(
    async (side: Side, path: string, size: number | null) => {
      const setter = side === 'A' ? setA : setB;
      setter({ path, size, info: null, rules: null, error: null });
      try {
        const info = await analyzeApk(path);
        let rules: RuleReport | null = null;
        if (selected.size > 0) {
          try {
            rules = await analyzeWithRules(path, Array.from(selected));
          } catch (e) {
            // Rule failures are non-fatal for the compare pipeline;
            // surface them but keep the analyze result.
            const msg = e instanceof Error ? e.message : String(e);
            setter((prev) => ({ ...prev, info, rules: null, error: msg }));
            toast.error(t('compare.rulesFailed', { side, error: msg }));
            return;
          }
        }
        setter({ path, size, info, rules, error: null });
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        setter({ path, size, info: null, rules: null, error: msg });
        toast.error(t('compare.sideFailed', { side, error: msg }));
      }
    },
    [selected, t],
  );

  async function pick(side: Side) {
    try {
      const p = await openDialog({
        multiple: false,
        filters: [{ name: 'APK', extensions: ['apk'] }],
      });
      if (typeof p !== 'string') return;
      // Best-effort file size so the picker row can show the user a
      // meaningful chip even before analysis completes. We delegate
      // to the same IPC Analyze uses.
      let size: number | null = null;
      try {
        const { fileSize } = await import('@/ipc/files');
        size = await fileSize(p);
      } catch {
        // non-fatal
      }
      await runForSide(side, p, size);
    } catch (e) {
      toast.error(String(e));
    }
  }

  function clear(side: Side) {
    if (side === 'A') setA(EMPTY_SIDE);
    else setB(EMPTY_SIDE);
  }

  function swap() {
    const tmp = a;
    setA(b);
    setB(tmp);
  }

  async function exportReport() {
    if (!a.info || !b.info) {
      toast.error(t('compare.pickBoth'));
      return;
    }
    if (!useLicenseStore.getState().requireFeature('apk_report_export')) return;
    try {
      const fileName = `compare-${a.info.package_name}_vs_${b.info.package_name}.html`;
      const html = buildCompareReportHtml({
        a: { info: a.info, rules: a.rules },
        b: { info: b.info, rules: b.rules },
      });
      const { save: saveDialog } = await import('@tauri-apps/plugin-dialog');
      const dest = await saveDialog({
        defaultPath: fileName,
        filters: [{ name: 'HTML Report', extensions: ['html'] }],
      });
      if (typeof dest !== 'string') return;
      const r = await exportApkReport({ dest_path: dest, html });
      toast.success(t('compare.exported', { path: r.dest_path }));
    } catch (e) {
      toast.error(t('compare.exportFailed', { error: String(e) }));
    }
  }

  const anyPicked = Boolean(a.path || b.path);
  const bothReady = Boolean(a.info && b.info);

  return (
    <div className="space-y-6 p-6">
      <header className="space-y-1">
        <h1 className="text-2xl font-semibold text-text-0">{t('compare.title')}</h1>
        <p className="text-sm text-text-2">{t('compare.subtitle')}</p>
      </header>

      {/* Picker row: APK A on the left, swap button in the middle, APK B on the right. */}
      <div className="grid gap-4 md:grid-cols-[1fr_auto_1fr]">
        <SidePicker
          side="A"
          data={a}
          onPick={() => void pick('A')}
          onClear={() => clear('A')}
        />
        <div className="flex items-center justify-center">
          <Button
            variant="ghost"
            size="icon"
            onClick={swap}
            disabled={!a.path && !b.path}
            title={t('compare.swap')}
          >
            <ArrowLeftRight className="h-4 w-4" />
          </Button>
        </div>
        <SidePicker
          side="B"
          data={b}
          onPick={() => void pick('B')}
          onClear={() => clear('B')}
        />
      </div>

      {/* Rule pack chooser — only shown if packs are available.
          Hidden when no packs means we still let the compare flow
          proceed with metadata-only diffs. */}
      {packs && packs.length > 0 && (
        <Card>
          <CardContent className="space-y-3">
            <div className="flex flex-wrap items-center gap-2 text-xs text-text-2">
              <span>{t('compare.rulesPacksAvailable', { count: packs.length })}</span>
              <span className="ml-auto text-text-2">
                {t('compare.rulesPacksHint')}
              </span>
            </div>
            <div className="flex flex-wrap gap-2">
              {packs.map((pack) => {
                const checked = selected.has(pack.id);
                return (
                  <label
                    key={pack.id}
                    className={cn(
                      'flex cursor-pointer items-center gap-2 rounded-md border px-2 py-1 text-xs',
                      checked
                        ? 'border-brand bg-brand/5'
                        : 'border-border bg-bg-1 hover:border-text-2',
                    )}
                  >
                    <input
                      type="checkbox"
                      className="h-3 w-3 accent-brand"
                      checked={checked}
                      onChange={() =>
                        setSelected((prev) => {
                          const next = new Set(prev);
                          if (next.has(pack.id)) next.delete(pack.id);
                          else next.add(pack.id);
                          return next;
                        })
                      }
                    />
                    <span className="truncate">{pack.name}</span>
                  </label>
                );
              })}
            </div>
          </CardContent>
        </Card>
      )}

      {/* Export button — only enabled once both sides are analyzed.
          VIP gating lives in `requireFeature` (see license.ts). */}
      {bothReady && (
        <div className="flex justify-end">
          <Button variant="outline" size="sm" onClick={() => void exportReport()}>
            <Download className="h-4 w-4" />
            {t('compare.exportVip')}
          </Button>
        </div>
      )}

      {!anyPicked && (
        <Card>
          <CardContent className="flex items-center gap-2 text-sm text-text-2">
            <Info className="h-4 w-4" />
            {t('compare.empty')}
          </CardContent>
        </Card>
      )}

      {bothReady && a.info && b.info && (
        <CompareDiff
          a={{ info: a.info, rules: a.rules, label: 'A' }}
          b={{ info: b.info, rules: b.rules, label: 'B' }}
        />
      )}
    </div>
  );
}

function SidePicker({
  side,
  data,
  onPick,
  onClear,
}: {
  side: Side;
  data: SideData;
  onPick: () => void;
  onClear: () => void;
}) {
  const { t } = useTranslation();
  const hasPath = Boolean(data.path);
  const isError = Boolean(data.error);
  const isAnalyzing = hasPath && !data.info && !isError;
  return (
    <Card className={cn(isError && 'border-danger/60')}>
      <CardContent className="space-y-3">
        <div className="flex items-center gap-2">
          <Badge variant={side === 'A' ? 'default' : 'secondary'}>
            {side === 'A' ? t('compare.left') : t('compare.right')}
          </Badge>
          {hasPath && data.size != null && (
            <Badge variant="outline">{formatBytes(data.size)}</Badge>
          )}
          {data.info && (
            <Badge variant="success">
              <CheckCircle2 className="mr-1 h-3 w-3" />
              {data.info.application_label?.trim() || data.info.package_name}
            </Badge>
          )}
          {isAnalyzing && (
            <Badge variant="secondary">
              <Loader2 className="mr-1 h-3 w-3 animate-spin" />
              {t('compare.analyzing')}
            </Badge>
          )}
          {isError && (
            <Badge variant="danger">
              <AlertTriangle className="mr-1 h-3 w-3" />
              {t('compare.errorTitle')}
            </Badge>
          )}
          {hasPath && (
            <Button
              variant="ghost"
              size="icon"
              className="ml-auto"
              onClick={onClear}
              title={t('compare.clear')}
            >
              <Eraser className="h-4 w-4" />
            </Button>
          )}
        </div>
        {hasPath && (
          <div
            className="truncate rounded-md border border-border bg-bg-2 px-2 py-1 font-mono text-xs text-text-1"
            title={data.path}
          >
            {data.path}
          </div>
        )}
        {isError && (
          <p className="break-all font-mono text-xs text-danger">{data.error}</p>
        )}
        <Button onClick={onPick} className="w-full">
          <FolderOpen className="h-4 w-4" />
          {side === 'A' ? t('compare.pickLeft') : t('compare.pickRight')}
        </Button>
      </CardContent>
    </Card>
  );
}

type SideEntry = {
  info: ApkInfo;
  rules: RuleReport | null;
  label: Side | string;
};

function CompareDiff({ a, b }: { a: SideEntry; b: SideEntry }) {
  const { t } = useTranslation();

  // Pre-compute each diff once and memo. The component already only
  // re-renders when `a` or `b` change, but memoising the diffs
  // makes the inner section components stable to React's identity
  // comparison when neither side has shifted (e.g. when re-rendering
  // for unrelated state in the parent).
  const permissions = useMemo(
    () => diffSets(a.info.permissions, b.info.permissions),
    [a.info.permissions, b.info.permissions],
  );

  const components = useMemo(() => {
    return {
      activities: diffSets(a.info.activities, b.info.activities),
      services: diffSets(a.info.services, b.info.services),
      receivers: diffSets(a.info.receivers, b.info.receivers),
      providers: diffSets(a.info.providers, b.info.providers),
    };
  }, [
    a.info.activities,
    a.info.services,
    a.info.receivers,
    a.info.providers,
    b.info.activities,
    b.info.services,
    b.info.receivers,
    b.info.providers,
  ]);

  const nativeLibs = useMemo(
    () => diffSets(a.info.native_libs ?? [], b.info.native_libs ?? []),
    [a.info.native_libs, b.info.native_libs],
  );

  // Rule-hit diffs compare metadata.label (the friendly library
  // name) when available, falling back to the raw component name.
  // Two apps that bundle the same SDK with slightly different class
  // names (rare but possible across forks) are still recognised as
  // "shared" via the label fallback.
  const rules = useMemo(() => {
    type Key = string;
    const collect = (report: RuleReport | null): Map<Key, { label: string; severity: string }> => {
      const out = new Map<Key, { label: string; severity: string }>();
      if (!report) return out;
      const categories = Object.values(report.components);
      for (const hits of categories) {
        for (const hit of hits) {
          if (!hit.matched_rule) continue;
          const label =
            hit.matched_rule.metadata?.label?.trim() || hit.name;
          out.set(label, {
            label,
            severity: (hit.matched_rule.severity ?? 'info').toLowerCase(),
          });
        }
      }
      return out;
    };
    const mapA = collect(a.rules);
    const mapB = collect(b.rules);
    const added: { label: string; severity: string }[] = [];
    const removed: { label: string; severity: string }[] = [];
    const shared: { label: string; severity: string }[] = [];
    for (const [k, v] of mapA) {
      if (mapB.has(k)) shared.push(v);
      else removed.push(v);
    }
    for (const [k, v] of mapB) {
      if (!mapA.has(k)) added.push(v);
    }
    added.sort((x, y) => x.label.localeCompare(y.label));
    removed.sort((x, y) => x.label.localeCompare(y.label));
    shared.sort((x, y) => x.label.localeCompare(y.label));
    return { added, removed, shared };
  }, [a.rules, b.rules]);

  return (
    <div className="space-y-4">
      <MetaSection a={a} b={b} />

      <DiffSection
        icon={ShieldCheck}
        title={t('compare.permissionsTitle')}
        diff={permissions}
        leftLabel={t('compare.left')}
        rightLabel={t('compare.right')}
        formatter={(p) => (
          <span className="font-mono text-xs text-text-1">{p}</span>
        )}
        addTitleKey="compare.permissionsAdded"
        removeTitleKey="compare.permissionsRemoved"
        sharedTitleKey="compare.permissionsShared"
      />

      <Card>
        <CardContent className="space-y-4">
          <div className="flex items-center gap-2">
            <Layers className="h-4 w-4 text-brand" />
            <span className="text-sm font-semibold text-text-0">
              {t('compare.componentsTitle')}
            </span>
          </div>
          {(
            [
              ['activities', 'compare.categoryActivities'],
              ['services', 'compare.categoryServices'],
              ['receivers', 'compare.categoryReceivers'],
              ['providers', 'compare.categoryProviders'],
            ] as const
          ).map(([key, labelKey]) => (
            <DiffSection
              key={key}
              icon={Server}
              title={t(labelKey)}
              diff={components[key]}
              leftLabel={t('compare.left')}
              rightLabel={t('compare.right')}
              formatter={(c) => (
                <span className="font-mono text-xs text-text-1">{c}</span>
              )}
              addTitleKey="compare.componentsAdded"
              removeTitleKey="compare.componentsRemoved"
              sharedTitleKey="compare.componentsShared"
              compact
            />
          ))}
        </CardContent>
      </Card>

      <DiffSection
        icon={Boxes}
        title={t('compare.nativeLibsTitle')}
        diff={nativeLibs}
        leftLabel={t('compare.left')}
        rightLabel={t('compare.right')}
        formatter={(lib) => (
          <span className="font-mono text-xs text-text-1">{lib}</span>
        )}
        addTitleKey="compare.componentsAdded"
        removeTitleKey="compare.componentsRemoved"
        sharedTitleKey="compare.componentsShared"
      />

      {packsBothPicked(a, b) && (
        <RuleDiffSection
          diff={rules}
          leftLabel={t('compare.left')}
          rightLabel={t('compare.right')}
        />
      )}
    </div>
  );
}

function packsBothPicked(a: SideEntry, b: SideEntry): boolean {
  return Boolean(a.rules || b.rules);
}

function MetaSection({ a, b }: { a: SideEntry; b: SideEntry }) {
  const { t } = useTranslation();
  const rows: Array<{
    label: string;
    left: React.ReactNode;
    right: React.ReactNode;
    changed: boolean;
  }> = [
    {
      label: t('compare.metaPackage'),
      left: a.info.package_name || '—',
      right: b.info.package_name || '—',
      changed: a.info.package_name !== b.info.package_name,
    },
    {
      label: t('compare.metaLabel'),
      left: a.info.application_label || '—',
      right: b.info.application_label || '—',
      changed: a.info.application_label !== b.info.application_label,
    },
    {
      label: t('compare.metaVersionName'),
      left: a.info.version_name || '—',
      right: b.info.version_name || '—',
      changed: a.info.version_name !== b.info.version_name,
    },
    {
      label: t('compare.metaVersionCode'),
      left: a.info.version_code || '—',
      right: b.info.version_code || '—',
      changed: a.info.version_code !== b.info.version_code,
    },
    {
      label: t('compare.metaSdk'),
      left: sdkSummary(a.info),
      right: sdkSummary(b.info),
      changed: sdkSummary(a.info) !== sdkSummary(b.info),
    },
    {
      label: t('compare.metaFileSize'),
      left: a.info.file_size != null ? formatBytes(a.info.file_size) : '—',
      right: b.info.file_size != null ? formatBytes(b.info.file_size) : '—',
      changed: a.info.file_size !== b.info.file_size,
    },
  ];
  return (
    <Card>
      <CardContent className="space-y-3">
        <div className="flex items-center gap-2">
          <PackageIcon className="h-4 w-4 text-brand" />
          <span className="text-sm font-semibold text-text-0">
            {t('compare.metaTitle')}
          </span>
        </div>
        <div className="grid grid-cols-[120px_1fr_1fr] gap-x-3 gap-y-2 text-sm">
          <div />
          <div className="text-xs font-medium uppercase tracking-wider text-text-2">
            {t('compare.left')}
          </div>
          <div className="text-xs font-medium uppercase tracking-wider text-text-2">
            {t('compare.right')}
          </div>
          {rows.map((row) => (
            <MetaRow key={row.label} {...row} />
          ))}
        </div>
      </CardContent>
    </Card>
  );
}

function MetaRow({
  label,
  left,
  right,
  changed,
}: {
  label: string;
  left: React.ReactNode;
  right: React.ReactNode;
  changed: boolean;
}) {
  return (
    <>
      <div className="flex items-center text-xs text-text-2">{label}</div>
      <ValueCell changed={changed}>{left}</ValueCell>
      <ValueCell changed={changed}>{right}</ValueCell>
    </>
  );
}

function ValueCell({
  changed,
  children,
}: {
  changed: boolean;
  children: React.ReactNode;
}) {
  return (
    <div
      className={cn(
        'min-w-0 truncate rounded-md border px-2 py-1 font-mono text-xs',
        changed
          ? 'border-warning/60 bg-warning/10 text-warning'
          : 'border-border bg-bg-1 text-text-1',
      )}
      title={typeof children === 'string' ? children : undefined}
    >
      {children}
    </div>
  );
}

function sdkSummary(info: ApkInfo): string {
  const parts: string[] = [];
  if (info.min_sdk) parts.push(`min ${info.min_sdk}`);
  if (info.target_sdk) parts.push(`target ${info.target_sdk}`);
  if (info.max_sdk) parts.push(`max ${info.max_sdk}`);
  return parts.length > 0 ? parts.join(' / ') : '—';
}

function DiffSection<T>({
  icon: Icon,
  title,
  diff,
  leftLabel,
  rightLabel,
  formatter,
  addTitleKey,
  removeTitleKey,
  sharedTitleKey,
  compact,
}: {
  icon: typeof Check;
  title: string;
  diff: DiffRow<T>;
  leftLabel: string;
  rightLabel: string;
  formatter: (item: T) => React.ReactNode;
  addTitleKey: string;
  removeTitleKey: string;
  sharedTitleKey: string;
  compact?: boolean;
}) {
  const { t } = useTranslation();
  const empty = diff.added.length === 0 && diff.removed.length === 0 && diff.shared.length === 0;
  if (empty) {
    return (
      <Card>
        <CardContent className="space-y-3">
          <div className="flex items-center gap-2">
            <Icon className="h-4 w-4 text-brand" />
            <span className="text-sm font-semibold text-text-0">{title}</span>
          </div>
          <p className="text-xs text-text-2">—</p>
        </CardContent>
      </Card>
    );
  }
  return (
    <Card>
      <CardContent className="space-y-3">
        <div className="flex items-center gap-2">
          <Icon className="h-4 w-4 text-brand" />
          <span className="text-sm font-semibold text-text-0">{title}</span>
        </div>
        <div className={cn('grid gap-3', compact ? 'md:grid-cols-1' : 'md:grid-cols-3')}>
          <DiffColumn
            kind="added"
            title={t(addTitleKey, { count: diff.added.length })}
            sub={t('compare.componentsAddedLabel', { side: rightLabel })}
            icon={Plus}
            items={diff.added}
            formatter={formatter}
          />
          <DiffColumn
            kind="removed"
            title={t(removeTitleKey, { count: diff.removed.length })}
            sub={t('compare.componentsRemovedLabel', { side: leftLabel })}
            icon={Minus}
            items={diff.removed}
            formatter={formatter}
          />
          <DiffColumn
            kind="shared"
            title={t(sharedTitleKey, { count: diff.shared.length })}
            sub={t('compare.componentsSharedLabel')}
            icon={Check}
            items={diff.shared}
            formatter={formatter}
          />
        </div>
      </CardContent>
    </Card>
  );
}

function DiffColumn<T>({
  kind,
  title,
  sub,
  icon: Icon,
  items,
  formatter,
}: {
  kind: 'added' | 'removed' | 'shared';
  title: string;
  sub: string;
  icon: typeof Plus;
  items: T[];
  formatter: (item: T) => React.ReactNode;
}) {
  const { t } = useTranslation();
  const palette = {
    added: 'border-success/40 bg-success/5',
    removed: 'border-danger/40 bg-danger/5',
    shared: 'border-border bg-bg-1',
  }[kind];
  const iconClass = {
    added: 'text-success',
    removed: 'text-danger',
    shared: 'text-text-2',
  }[kind];
  return (
    <div className={cn('rounded-lg border p-3', palette)}>
      <div className="mb-2 flex items-center gap-2 text-xs font-medium">
        <Icon className={cn('h-3.5 w-3.5', iconClass)} />
        <span className="text-text-0">{title}</span>
      </div>
      {items.length === 0 ? (
        <p className="text-[11px] text-text-2">—</p>
      ) : (
        <>
          <p className="mb-1 text-[11px] text-text-2">{sub}</p>
          <ul className="max-h-72 space-y-1 overflow-auto">
            {items.map((item, i) => (
              <li
                key={i}
                className="flex items-baseline gap-2 rounded-md border border-border bg-bg-1 px-2 py-1"
              >
                <FileSearch className="h-3 w-3 shrink-0 text-text-2" />
                <span className="min-w-0 flex-1 truncate" title={String(item)}>
                  {formatter(item)}
                </span>
              </li>
            ))}
          </ul>
          {items.length > 0 && (
            <p className="mt-1 text-[11px] text-text-2">
              {t('compare.matchedCount', { count: items.length })}
            </p>
          )}
        </>
      )}
    </div>
  );
}

function RuleDiffSection({
  diff,
  leftLabel,
  rightLabel,
}: {
  diff: { added: { label: string; severity: string }[]; removed: { label: string; severity: string }[]; shared: { label: string; severity: string }[] };
  leftLabel: string;
  rightLabel: string;
}) {
  const { t } = useTranslation();
  const sevVariant = (sev: string) => {
    if (sev === 'danger') return 'danger' as const;
    if (sev === 'warn' || sev === 'warning') return 'warning' as const;
    return 'secondary' as const;
  };
  const column = (
    items: { label: string; severity: string }[],
    icon: typeof Plus,
    palette: string,
    iconClass: string,
    titleKey: string,
    sub: string,
    emptyHint: string,
  ) => (
    <div className={cn('rounded-lg border p-3', palette)}>
      <div className="mb-2 flex items-center gap-2 text-xs font-medium">
        {icon === Plus ? (
          <Plus className={cn('h-3.5 w-3.5', iconClass)} />
        ) : icon === Minus ? (
          <Minus className={cn('h-3.5 w-3.5', iconClass)} />
        ) : (
          <Check className={cn('h-3.5 w-3.5', iconClass)} />
        )}
        <span className="text-text-0">{t(titleKey, { count: items.length })}</span>
      </div>
      {items.length === 0 ? (
        <p className="text-[11px] text-text-2">{emptyHint}</p>
      ) : (
        <>
          <p className="mb-1 text-[11px] text-text-2">{sub}</p>
          <ul className="max-h-72 space-y-1 overflow-auto">
            {items.map((it, i) => (
              <li
                key={i}
                className="flex items-center gap-2 rounded-md border border-border bg-bg-1 px-2 py-1"
              >
                <span className="min-w-0 flex-1 truncate text-xs text-text-0">
                  {it.label}
                </span>
                <Badge variant={sevVariant(it.severity)} className="shrink-0 text-[10px]">
                  {it.severity}
                </Badge>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
  return (
    <Card>
      <CardContent className="space-y-3">
        <div className="flex items-center gap-2">
          <FileSearch className="h-4 w-4 text-brand" />
          <span className="text-sm font-semibold text-text-0">
            {t('compare.rulesTitle')}
          </span>
        </div>
        <div className="grid gap-3 md:grid-cols-3">
          {column(
            diff.added,
            Plus,
            'border-success/40 bg-success/5',
            'text-success',
            'compare.rulesAdded',
            t('compare.componentsAddedLabel', { side: rightLabel }),
            '—',
          )}
          {column(
            diff.removed,
            Minus,
            'border-danger/40 bg-danger/5',
            'text-danger',
            'compare.rulesRemoved',
            t('compare.componentsRemovedLabel', { side: leftLabel }),
            '—',
          )}
          {column(
            diff.shared,
            Check,
            'border-border bg-bg-1',
            'text-text-2',
            'compare.rulesShared',
            t('compare.componentsSharedLabel'),
            '—',
          )}
        </div>
      </CardContent>
    </Card>
  );
}
