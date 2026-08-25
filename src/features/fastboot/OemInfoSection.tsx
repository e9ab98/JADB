import { useTranslation } from 'react-i18next';
import '@/i18n';
import {
  AlertTriangle,
  ChevronDown,
  Loader2,
  RefreshCw,
  Wrench,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';
import { detectOem, parseOemDeviceInfo, type OemId, type ParsedOemFieldKey } from './oemParsers';

/**
 * Per-device OEM diagnostics section. Rendered at the bottom of the
 * FastbootInfoPanel grid when the panel is expanded.
 *
 * State machine is owned by the parent (`FastbootPanel`) so the
 * fetched payload is cached per serial — re-opening the info panel
 * for a device we already queried doesn't refetch.
 *
 *  - `idle`     — parent hasn't triggered a fetch yet
 *  - `loading`  — `fastboot oem device-info` is in flight
 *  - `loaded`   — raw lines arrived; we route through OEM parser here
 *  - `unsupported` — backend errored (bootloader doesn't implement
 *    the command). The section renders a muted "not supported" line
 *    with a Retry button, since rebooting into a different bootloader
 *    state can flip the support flag.
 */
export type OemSectionState =
  | { status: 'idle' }
  | { status: 'loading' }
  | { status: 'loaded'; rawLines: string[] }
  | { status: 'unsupported'; message: string };

export function OemInfoSection({
  state,
  product,
  onRetry,
}: {
  state: OemSectionState;
  product: string | null;
  onRetry: () => void;
}) {
  const { t } = useTranslation();
  if (state.status === 'idle') return null;

  const oem: OemId = detectOem(product);
  const oemLabel =
    oem === 'pixel' ? t('fastboot.oemOemPixel')
    : oem === 'xiaomi' ? t('fastboot.oemOemXiaomi')
    : oem === 'samsung' ? t('fastboot.oemOemSamsung')
    : t('fastboot.oemOemUnknown');

  return (
    <div className="mt-3 border-t border-border bg-bg-0/40 px-3 pb-3 pt-2">
      <div className="mb-2 flex flex-wrap items-center gap-2">
        <div className="flex items-center gap-2 text-xs font-semibold text-text-0">
          <Wrench className="h-3 w-3" />
          {t('fastboot.oemSection')}
        </div>
        <Badge variant="secondary">{oemLabel}</Badge>
        <span className="text-[10px] text-text-2">
          {t('fastboot.oemHint')}
        </span>
      </div>

      {state.status === 'loading' && (
        <div className="flex items-center gap-2 px-1 py-2 text-xs text-text-2">
          <Loader2 className="h-3 w-3 animate-spin" />
          {t('fastboot.oemLoading')}
        </div>
      )}

      {state.status === 'unsupported' && (
        <div className="flex items-center justify-between gap-2 px-1 py-2 text-xs">
          <div className="flex items-center gap-2 text-text-2">
            <AlertTriangle className="h-3 w-3" />
            {t('fastboot.oemUnsupported')}
            {state.message && state.message !== 'empty' && (
              <span className="font-mono">· {state.message}</span>
            )}
          </div>
          <Button size="sm" variant="outline" onClick={onRetry}>
            <RefreshCw className="h-3 w-3" />
            {t('fastboot.oemRetry')}
          </Button>
        </div>
      )}

      {state.status === 'loaded' && (
        <LoadedOemContent
          rawLines={state.rawLines}
          oem={oem}
          onRetry={onRetry}
        />
      )}
    </div>
  );
}

/**
 * Render the `loaded` branch. We always run the parser even on
 * non-Pixel / non-Xiaomi OEMs so any `Key: value` lines get parsed
 * and only truly-unrecognised lines fall into the collapsible. This
 * means future OEMs that adopt Pixel's `(bootloader) Key: value`
 * shape will Just Work without a parser addition.
 */
function LoadedOemContent({
  rawLines,
  oem,
  onRetry,
}: {
  rawLines: string[];
  oem: OemId;
  onRetry: () => void;
}) {
  const { t } = useTranslation();
  if (rawLines.length === 0) {
    return (
      <div className="flex items-center justify-between gap-2 px-1 py-2 text-xs">
        <div className="flex items-center gap-2 text-text-2">
          <AlertTriangle className="h-3 w-3" />
          {t('fastboot.oemEmpty')}
        </div>
        <Button size="sm" variant="outline" onClick={onRetry}>
          <RefreshCw className="h-3 w-3" />
          {t('fastboot.oemRetry')}
        </Button>
      </div>
    );
  }

  const parsed = parseOemDeviceInfo(rawLines);
  // Hide cells for keys the bootloader didn't report rather than
  // render them as "N/A" — a missing key in oem device-info is
  // semantically different from "unknown value".
  const cellDefs: Array<{ key: ParsedOemFieldKey; label: string; successIsYes: boolean }> = [
    { key: 'Device tampered',          label: t('fastboot.oemDeviceTampered'),          successIsYes: false },
    { key: 'Device unlocked',          label: t('fastboot.oemDeviceUnlocked'),          successIsYes: true },
    { key: 'Device critical unlocked', label: t('fastboot.oemDeviceCriticalUnlocked'), successIsYes: true },
    { key: 'Charger screen bypass',    label: t('fastboot.oemChargerBypass'),           successIsYes: true },
  ];
  const visibleCells = cellDefs.filter((c) => parsed.fields[c.key] !== undefined);
  return (
    <div className="space-y-2">
      {visibleCells.length > 0 && (
        <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
          {visibleCells.map((c) => (
            <OemFieldCell
              key={c.key}
              label={c.label}
              raw={parsed.fields[c.key] as string}
              successIsYes={c.successIsYes}
            />
          ))}
        </div>
      )}
      {parsed.unrecognized.length > 0 && (
        <details className="rounded-md border border-border bg-bg-0 px-3 py-2 text-xs">
          <summary className="flex cursor-pointer items-center justify-between text-text-1">
            <span className="flex items-center gap-1.5">
              <ChevronDown className="h-3 w-3" />
              {t('fastboot.oemOtherFields')}
              <span className="text-text-2">
                ({t('fastboot.oemOtherFieldsHint', { count: parsed.unrecognized.length })})
              </span>
            </span>
            {oem === 'unknown' && (
              <Badge variant="secondary">{t('fastboot.oemOemUnknown')}</Badge>
            )}
          </summary>
          <pre className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap break-all font-mono text-[11px] text-text-2">
            {parsed.unrecognized.join('\n')}
          </pre>
        </details>
      )}
      {parsed.unrecognized.length === 0 && visibleCells.length === 0 && (
        <div className="flex items-center gap-2 px-1 py-2 text-xs text-text-2">
          <AlertTriangle className="h-3 w-3" />
          {t('fastboot.oemEmpty')}
        </div>
      )}
    </div>
  );
}

function OemFieldCell({
  label,
  raw,
  successIsYes,
}: {
  label: string;
  raw: string;
  successIsYes: boolean;
}) {
  const { t } = useTranslation();
  // oem device-info returns literal yes/no on Pixel, but some
  // Xiaomi bootloaders return "true"/"false" or 1/0 — be lenient.
  const lower = raw.toLowerCase().trim();
  const isYes = lower === 'yes' || lower === 'true' || lower === '1';
  const isNo = lower === 'no' || lower === 'false' || lower === '0';
  if (!isYes && !isNo) {
    return (
      <div className="rounded-md border border-border bg-bg-1 px-3 py-2">
        <div className="text-[10px] font-medium uppercase tracking-wide text-text-2">
          {label}
        </div>
        <div className="mt-1 font-mono text-sm text-text-0">{raw}</div>
      </div>
    );
  }
  // "Device tampered" success = `no`. Others success = `yes`.
  const isSuccess = successIsYes ? isYes : isNo;
  return (
    <div className="rounded-md border border-border bg-bg-1 px-3 py-2">
      <div className="text-[10px] font-medium uppercase tracking-wide text-text-2">
        {label}
      </div>
      <div className="mt-1">
        <span
          className={cn(
            'inline-flex items-center gap-1 rounded-md px-2 py-0.5 text-xs font-semibold',
            isSuccess ? 'bg-success/15 text-success' : 'bg-danger/15 text-danger',
          )}
        >
          {isYes ? t('fastboot.varValueYes') : t('fastboot.varValueNo')}
        </span>
      </div>
    </div>
  );
}
