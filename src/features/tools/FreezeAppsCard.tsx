import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import {
  CheckCircle2,
  Circle,
  Loader2,
  Lock,
  LockOpen,
  RefreshCw,
  Search,
  Snowflake,
  XCircle,
} from 'lucide-react';
import { toast } from 'sonner';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { adbListPackages, adbShell } from '@/ipc/adb';
import { cn } from '@/lib/utils';

/**
 * Bulk-freeze / unfreeze tool backed by `pm disable-user` /
 * `pm enable`.
 *
 * `disable-user` keeps the APK on the device but disables the
 * launcher component so the app icon vanishes; this is the
 * classic "freeze" workflow used by OEM debloat guides. Unlike
 * `pm disable`, `disable-user` survives a factory reset only if
 * the user is 0 -- which is always the case on a single-user
 * phone, so the two are equivalent in practice.
 *
 * We deliberately do NOT touch system packages by default -- many
 * OEMs (notably MIUI and HarmonyOS) will hard-brick the launcher
 * if you disable the wrong `com.android.systemui` sibling. Users
 * can opt in via the "show system" toggle if they really know
 * what they're doing.
 */
type Row = {
  pkg: string;
  /** "disabled" comes from `pm list packages -d` (the freeze
   *  set); everything else is treated as "enabled". */
  disabled: boolean;
};

type Filter = 'all' | 'disabled' | 'enabled';

function parsePmList(stdout: string): string[] {
  // `pm list packages -d` returns one `package:<name>` per line.
  // We trim and strip the prefix; invalid lines are skipped so a
  // device with a misbehaving shell doesn't blow up the UI.
  const out: string[] = [];
  for (const line of stdout.split(/\r?\n/)) {
    const m = /^package:(\S+)$/.exec(line.trim());
    if (m && m[1]) out.push(m[1]);
  }
  return out;
}

export function FreezeAppsCard({ serial }: { serial: string }) {
  const { t } = useTranslation();
  const [rows, setRows] = useState<Row[]>([]);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState<'freeze' | 'unfreeze' | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<Filter>('all');
  const [includeSystem, setIncludeSystem] = useState(false);

  async function refresh() {
    if (!serial) return;
    setLoading(true);
    try {
      const [pkgs, disabledOut] = await Promise.all([
        adbListPackages(serial, includeSystem),
        // `pm list packages -d` lists everything that's been
        // `disable-user`d (and `disable`d, but those typically
        // can't be re-enabled by a non-root user). The
        // distinction matters: a frozen-by-system app can't be
        // re-enabled from the UI, which we'll surface via the
        // badge so the user doesn't try.
        adbShell(serial, 'pm list packages -d'),
      ]);
      const disabled = new Set(disabledOut ? parsePmList(disabledOut.stdout) : []);
      setRows(pkgs.map((pkg) => ({ pkg, disabled: disabled.has(pkg) })));
      // Drop any previously-selected rows that aren't in the
      // refreshed list (the user uninstalled them mid-session).
      setSelected((prev) => {
        const next = new Set<string>();
        for (const pkg of prev) if (pkgs.includes(pkg)) next.add(pkg);
        return next;
      });
    } catch (e) {
      toast.error(String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void refresh();
    // Re-fetch when the system filter flips so the user doesn't
    // have to click Refresh manually.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serial, includeSystem]);

  // Filter + search compose: filter narrows by state, query
  // narrows by substring (matches pkg only -- we don't have
  // app labels without a `dumpsys` per row, which would be too
  // slow for a list view).
  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return rows.filter((r) => {
      if (filter === 'disabled' && !r.disabled) return false;
      if (filter === 'enabled' && r.disabled) return false;
      if (q && !r.pkg.toLowerCase().includes(q)) return false;
      return true;
    });
  }, [rows, query, filter]);

  function toggleOne(pkg: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(pkg)) next.delete(pkg);
      else next.add(pkg);
      return next;
    });
  }

  function toggleAllVisible() {
    setSelected((prev) => {
      // Select-all-visible = add all visible; if everything visible
      // is already selected, clear. The "visible" qualifier
      // prevents the user from accidentally selecting 200 packages
      // by clicking once after a filter.
      const allSelected = filtered.length > 0 && filtered.every((r) => prev.has(r.pkg));
      if (allSelected) return new Set();
      const next = new Set(prev);
      for (const r of filtered) next.add(r.pkg);
      return next;
    });
  }

  async function applyBulk(action: 'freeze' | 'unfreeze') {
    if (selected.size === 0 || busy) return;
    const pkgs = [...selected];
    setBusy(action);
    // Track per-package result so the user can see which ones
    // failed (e.g. system packages that need root to re-enable).
    const failed: string[] = [];
    for (const pkg of pkgs) {
      try {
        const cmd =
          action === 'freeze'
            ? `pm disable-user --user 0 ${pkg}`
            : `pm enable ${pkg}`;
        const out = await adbShell(serial, cmd);
        const ok = !!out && out.exitCode === 0;
        if (!ok) failed.push(pkg);
        // Optimistically reflect the new state for the next render;
        // a failed row will get corrected by `refresh()` below.
        setRows((prev) =>
          prev.map((r) => (r.pkg === pkg ? { ...r, disabled: action === 'freeze' } : r)),
        );
      } catch {
        failed.push(pkg);
      }
    }
    setBusy(null);
    if (failed.length > 0) {
      toast.error(t('tools.freezeApps.partialFail', { count: failed.length }));
    } else {
      toast.success(
        action === 'freeze'
          ? t('tools.freezeApps.frozenOk', { count: pkgs.length })
          : t('tools.freezeApps.unfrozenOk', { count: pkgs.length }),
      );
    }
    // Re-fetch authoritative state so we catch the rows we
    // optimistically flipped wrong.
    await refresh();
  }

  // Refs for the filter pills so we can style the active one.
  const filters: { key: Filter; label: string }[] = [
    { key: 'all', label: t('tools.freezeApps.filterAll') },
    { key: 'enabled', label: t('tools.freezeApps.filterEnabled') },
    { key: 'disabled', label: t('tools.freezeApps.filterDisabled') },
  ];

  const allVisibleSelected = filtered.length > 0 && filtered.every((r) => selected.has(r.pkg));
  const initialMount = useRef(true);
  // Don't show the empty-state placeholder until after the first
  // load completes; otherwise the user sees "no packages" briefly
  // before the IPC resolves.
  const [hasLoaded, setHasLoaded] = useState(false);
  useEffect(() => {
    if (initialMount.current) {
      initialMount.current = false;
      return;
    }
    if (!loading) setHasLoaded(true);
  }, [loading]);

  return (
    <Card className="overflow-hidden">
      <CardContent className="space-y-2 p-3">
        <div className="flex items-center gap-2.5">
          <div className="grid h-8 w-8 shrink-0 place-items-center rounded-md bg-brand/15 text-brand">
            <Snowflake className="h-4 w-4" />
          </div>
          <div className="min-w-0 flex-1">
            <h3 className="truncate text-sm font-medium leading-tight text-text-0">
              {t('tools.freezeApps.title')}
            </h3>
            <p className="truncate text-[11px] leading-tight text-text-2">
              {t('tools.freezeApps.subtitle')}
            </p>
          </div>
          <Button
            variant="outline"
            size="sm"
            className="h-7 gap-1 px-2"
            onClick={() => void refresh()}
            disabled={loading}
          >
            <RefreshCw className={cn('h-3.5 w-3.5', loading && 'animate-spin')} />
          </Button>
        </div>

        <div className="flex items-center gap-2">
          <div className="relative min-w-0 flex-1">
            <Search className="pointer-events-none absolute left-2 top-1/2 h-3 w-3 -translate-y-1/2 text-text-2" />
            <Input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t('tools.freezeApps.searchPlaceholder')}
              className="h-7 pl-7 text-xs"
            />
          </div>
          <div className="flex shrink-0 items-center gap-1 rounded-md border border-border bg-bg-2 p-0.5 text-[11px]">
            {filters.map((f) => (
              <button
                key={f.key}
                type="button"
                onClick={() => setFilter(f.key)}
                className={cn(
                  'rounded px-1.5 py-0.5 transition-colors',
                  filter === f.key
                    ? 'bg-bg-1 text-text-0 shadow-button'
                    : 'text-text-2 hover:text-text-1',
                )}
              >
                {f.label}
              </button>
            ))}
          </div>
        </div>

        <label className="flex cursor-pointer items-center gap-1.5 text-[11px] text-text-2">
          <input
            type="checkbox"
            checked={includeSystem}
            onChange={(e) => setIncludeSystem(e.target.checked)}
            className="h-3 w-3 accent-brand"
          />
          {t('tools.freezeApps.includeSystem')}
        </label>

        <div className="flex items-center justify-between text-[11px] text-text-2">
          <span>
            {t('tools.freezeApps.selectedCount', {
              selected: selected.size,
              total: filtered.length,
            })}
          </span>
          {filtered.length > 0 && (
            <button
              type="button"
              onClick={toggleAllVisible}
              className="text-brand hover:underline"
            >
              {allVisibleSelected
                ? t('tools.freezeApps.deselectAll')
                : t('tools.freezeApps.selectAllVisible')}
            </button>
          )}
        </div>

        <ScrollArea className="h-56 rounded-md border border-border bg-bg-2/40">
          {loading && rows.length === 0 ? (
            <div className="flex h-full items-center justify-center text-xs text-text-2">
              <Loader2 className="mr-1 h-3 w-3 animate-spin" />
              {t('tools.freezeApps.loading')}
            </div>
          ) : filtered.length === 0 ? (
            <div className="flex h-full items-center justify-center text-xs text-text-2">
              {hasLoaded ? t('tools.freezeApps.empty') : t('tools.freezeApps.loading')}
            </div>
          ) : (
            <ul className="divide-y divide-border">
              {filtered.map((row) => {
                const checked = selected.has(row.pkg);
                return (
                  <li
                    key={row.pkg}
                    className="flex cursor-pointer items-center gap-2 px-2 py-1 text-xs hover:bg-bg-1"
                    onClick={() => toggleOne(row.pkg)}
                  >
                    {checked ? (
                      <CheckCircle2 className="h-3.5 w-3.5 shrink-0 text-brand" />
                    ) : (
                      <Circle className="h-3.5 w-3.5 shrink-0 text-text-2" />
                    )}
                    <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-text-1">
                      {row.pkg}
                    </span>
                    {row.disabled ? (
                      <Badge variant="secondary" className="h-4 px-1 py-0 text-[10px] leading-none">
                        {t('tools.freezeApps.badgeDisabled')}
                      </Badge>
                    ) : (
                      <Badge variant="success" className="h-4 px-1 py-0 text-[10px] leading-none">
                        {t('tools.freezeApps.badgeEnabled')}
                      </Badge>
                    )}
                  </li>
                );
              })}
            </ul>
          )}
        </ScrollArea>

        <div className="flex items-center justify-end gap-2">
          <Button
            variant="outline"
            size="sm"
            className="h-7 gap-1 px-2.5"
            disabled={selected.size === 0 || busy !== null}
            onClick={() => void applyBulk('unfreeze')}
          >
            {busy === 'unfreeze' ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <LockOpen className="h-3.5 w-3.5" />
            )}
            {t('tools.freezeApps.unfreeze')}
            {selected.size > 0 && <span className="text-text-2">({selected.size})</span>}
          </Button>
          <Button
            size="sm"
            className="h-7 gap-1 px-2.5"
            disabled={selected.size === 0 || busy !== null}
            onClick={() => void applyBulk('freeze')}
          >
            {busy === 'freeze' ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Lock className="h-3.5 w-3.5" />
            )}
            {t('tools.freezeApps.freeze')}
            {selected.size > 0 && <span className="text-text-2">({selected.size})</span>}
          </Button>
        </div>
        {busy && (
          <div className="flex items-center gap-1 text-[11px] text-text-2">
            <Loader2 className="h-3 w-3 animate-spin" />
            {t('tools.freezeApps.busy', { count: selected.size })}
          </div>
        )}
        {/* Tiny visual hint when the user selected 0 packages -- the
            action buttons already disable, but a textual reminder
            avoids confusion. Hidden once the first selection lands. */}
        {!busy && selected.size === 0 && hasLoaded && rows.length > 0 && (
          <div className="flex items-center gap-1 text-[10px] text-text-2">
            <XCircle className="h-3 w-3" />
            {t('tools.freezeApps.hintSelect')}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
