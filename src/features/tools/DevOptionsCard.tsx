import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import {
  Loader2,
  Settings2,
} from 'lucide-react';
import { toast } from 'sonner';
import { Card, CardContent } from '@/components/ui/card';
import { Switch } from '@/components/ui/switch';
import { safeShell } from './_shared/safeShell';

/**
 * Developer-options switch matrix.
 *
 * Why this exists: the in-built developer-options screen is buried
 * behind ~5 taps (Settings -> About phone -> tap Build 7 times ->
 * System -> Developer options) and individual switches are scattered
 * across half a dozen scroll positions. This card collapses the
 * ones most commonly toggled during a debug session into one panel
 * and exposes them as plain boolean switches.
 *
 * Each switch maps to one `settings put global/system` key (or
 * `setprop debug.*` for the layout-bounds row, which lives in the
 * `debug.*` namespace instead of `Settings`).
 *
 * - Animation-scale rows (`window_animation_scale`,
 *   `transition_animation_scale`, `animator_duration_scale`) flip
 *   between `1` (stock) and `0.5` (faster). The other axes the
 *   stock UI exposes (1.5x, 10x, off) are intentionally not
 *   exposed -- the matrix is for "make the phone feel snappy" /
 *   "restore stock", nothing fancier.
 * - The remaining rows are pure booleans.
 *
 * State model: we snapshot the current value once on mount (so the
 * switch reflects reality instead of being optimistically "off")
 * and update it again after every successful `settings put`.
 */
type ToggleRow = {
  /** Stable id used as React key + i18n suffix. */
  id: string;
  /** Where the value lives. `global`/`system` go through
   *  `settings put`; `prop` goes through `setprop debug.<key>`
   *  (a few developer-options like layout-bounds live there). */
  scope: 'global' | 'system' | 'prop';
  /** Setting name (or prop suffix for `scope: 'prop'`). */
  keyName: string;
  /** The "on" value to put. The "off" value is hard-coded below
   *  based on `scope` (1 for booleans, 0.5 for animation scales). */
  onValue: string;
  /** True if `id` is one of the three animation-scale rows. */
  isAnimationScale: boolean;
};

const TOGGLES: ToggleRow[] = [
  { id: 'windowAnimScale', scope: 'global', keyName: 'window_animation_scale', onValue: '0.5', isAnimationScale: true },
  { id: 'transitionAnimScale', scope: 'global', keyName: 'transition_animation_scale', onValue: '0.5', isAnimationScale: true },
  { id: 'animatorScale', scope: 'global', keyName: 'animator_duration_scale', onValue: '0.5', isAnimationScale: true },
  { id: 'forceGpu', scope: 'global', keyName: 'force_hw_ui', onValue: '1', isAnimationScale: false },
  { id: 'showTouches', scope: 'system', keyName: 'show_touches', onValue: '1', isAnimationScale: false },
  { id: 'alwaysFinishActivities', scope: 'global', keyName: 'always_finish_activities', onValue: '1', isAnimationScale: false },
  { id: 'strictMode', scope: 'global', keyName: 'strict_mode', onValue: '1', isAnimationScale: false },
  { id: 'layoutBounds', scope: 'prop', keyName: 'debug.layout', onValue: '1', isAnimationScale: false },
];

// "Off" target value differs by row family: animation-scale rows go
// back to `1` (stock), everything else goes to `0` (disabled).
function offValue(row: ToggleRow): string {
  return row.isAnimationScale ? '1' : '0';
}

export function DevOptionsCard({ serial }: { serial: string }) {
  const { t } = useTranslation();
  // `state[id]` is the live boolean we render in the switch.
  // `null` means "haven't queried yet" -- we don't render an empty
  // switch while loading, just a small spinner instead.
  const [state, setState] = useState<Record<string, boolean | null>>(
    Object.fromEntries(TOGGLES.map((row) => [row.id, null])),
  );
  // Track which row is mid-write so the UI can disable the row's
  // switch and show a tiny spinner next to the label. Avoids the
  // user double-clicking while the previous write is in flight.
  const [pending, setPending] = useState<Record<string, boolean>>(
    Object.fromEntries(TOGGLES.map((row) => [row.id, false])),
  );

  useEffect(() => {
    let alive = true;
    async function load() {
      const next: Record<string, boolean | null> = {};
      for (const row of TOGGLES) {
        try {
          const cmd =
            row.scope === 'prop'
              ? `getprop ${row.keyName}`
              : `settings get ${row.scope} ${row.keyName}`;
          const out = await safeShell(serial, cmd);
          if (!alive) return;
          const v = out?.stdout.trim() ?? '';
          // `settings get` returns `null` (literal string) for
          // unset values; coerce to false in that case.
          next[row.id] = v === row.onValue;
        } catch {
          if (!alive) return;
          next[row.id] = null;
        }
      }
      if (alive) setState(next);
    }
    void load();
    return () => {
      alive = false;
    };
  }, [serial]);

  async function toggle(row: ToggleRow, next: boolean) {
    if (pending[row.id]) return;
    const target = next ? row.onValue : offValue(row);
    setPending((p) => ({ ...p, [row.id]: true }));
    try {
      const cmd =
        row.scope === 'prop'
          ? `setprop ${row.keyName} ${target}`
          : `settings put ${row.scope} ${row.keyName} ${target}`;
      const out = await safeShell(serial, cmd);
      if (!out || out.exitCode !== 0) {
        toast.error(
          t('tools.runFailed', {
            error: out?.stderr.trim() || `settings put failed`,
          }),
        );
        return;
      }
      setState((s) => ({ ...s, [row.id]: next }));
    } catch (e) {
      toast.error(t('tools.runFailed', { error: String(e) }));
    } finally {
      setPending((p) => ({ ...p, [row.id]: false }));
    }
  }

  return (
    <Card className="overflow-hidden">
      <CardContent className="p-3">
        <div className="flex items-center gap-2.5">
          <div className="grid h-8 w-8 shrink-0 place-items-center rounded-md bg-brand/15 text-brand">
            <Settings2 className="h-4 w-4" />
          </div>
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-1.5">
              <h3 className="truncate text-sm font-medium leading-tight text-text-0">
                {t('tools.devOptions.title')}
              </h3>
            </div>
            <p className="truncate text-[11px] leading-tight text-text-2">
              {t('tools.devOptions.subtitle')}
            </p>
          </div>
        </div>

        {/* Two-column grid on `md+` to fit 8 rows in ~4 lines without
            scrolling; single column on mobile. Each row is its own
            label + switch so screen readers and keyboard users
            can navigate them individually. */}
        <div className="mt-3 grid grid-cols-1 gap-1.5 md:grid-cols-2">
          {TOGGLES.map((row) => {
            const value = state[row.id];
            const isPending = pending[row.id];
            return (
              <label
                key={row.id}
                htmlFor={`devop-${row.id}`}
                className="flex cursor-pointer items-center gap-2 rounded-md border border-border bg-bg-2/40 px-2 py-1.5 transition-colors hover:bg-bg-2"
              >
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-1.5 text-[12px] leading-tight text-text-0">
                    {t(`tools.devOptions.toggles.${row.id}`)}
                    {isPending && (
                      <Loader2 className="h-3 w-3 animate-spin text-text-2" />
                    )}
                  </div>
                  <div className="truncate text-[10px] leading-tight text-text-2">
                    {t(`tools.devOptions.descriptions.${row.id}`)}
                  </div>
                </div>
                <Switch
                  id={`devop-${row.id}`}
                  checked={value === true}
                  disabled={value === null || isPending}
                  onCheckedChange={(v) => void toggle(row, v)}
                />
              </label>
            );
          })}
        </div>
      </CardContent>
    </Card>
  );
}
