import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import { toast } from 'sonner';
import {
  AlertTriangle,
  Check,
  ChevronDown,
  Info,
  Loader2,
  RefreshCw,
  RotateCcw,
  Smartphone,
  Terminal,
  X,
  Zap,
} from 'lucide-react';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  fastbootDevices,
  fastbootGetInfo,
  fastbootGetOemDeviceInfo,
  fastbootReboot,
  type FastbootDevice,
  type FastbootVarInfo,
} from '@/ipc/fastboot';
import { useSettingsStore } from '@/store/settings';
import { cn } from '@/lib/utils';
import {
  OemInfoSection,
  type OemSectionState,
} from './OemInfoSection';

/** Sentinel prefix used by `AppError::ToolMissing("fastboot")` once
 *  serialized to a Tauri error string. Used by the panel to render the
 *  "install Platform-Tools" banner instead of a generic failure. */
const FASTBOOT_MISSING_PREFIX = 'tool missing: fastboot';

/**
 * Detect whether an IPC error means the fastboot binary is missing
 * (vs. a transient command failure). The Rust layer's
 * `AppError::ToolMissing("fastboot")` serializes as the literal string
 * `"tool missing: fastboot"` — we match on that prefix.
 */
function isFastbootMissing(message: string): boolean {
  return message.includes(FASTBOOT_MISSING_PREFIX);
}

/**
 * Fastboot tab content. Mirrors `AdbConnectionPanel`'s structure —
 * header card with refresh + device count, body card listing devices,
 * 5-second polling so a freshly-attached USB device shows up without
 * a manual reload. Each device row exposes the three safe fastboot
 * operations: reboot to system / recovery / bootloader. No flash /
 * erase / unlock — those belong to a separate confirm-heavy workflow.
 */
export function FastbootPanel() {
  const { t } = useTranslation();
  const adbPath = useSettingsStore((s) => s.settings?.adbPath ?? null);
  const [devices, setDevices] = useState<FastbootDevice[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [fastbootMissingFlag, setFastbootMissingFlag] = useState(false);
  const [busySerial, setBusySerial] = useState<string | null>(null);
  // Per-device info panel state. `idle` means the toggle has never been
  // opened; `loading` means a getvar fetch is in flight; `loaded`
  // caches the last successful response; `error` records the failure
  // message so the user can retry without a fresh mount.
  type InfoState =
    | { status: 'idle' }
    | { status: 'loading' }
    | { status: 'loaded'; data: FastbootVarInfo }
    | { status: 'error'; message: string };
  const [infoBySerial, setInfoBySerial] = useState<Record<string, InfoState>>({});
  const [openInfoSerial, setOpenInfoSerial] = useState<string | null>(null);
  // Cached per-serial `oem device-info` state. Separate from
  // infoBySerial because (a) it has different states (unsupported
  // vs. error) and (b) the lifecycle is independent — a device can
  // expose vars but not oem device-info, or vice versa.
  const [oemBySerial, setOemBySerial] = useState<Record<string, OemSectionState>>({});

  async function refresh() {
    if (!adbPath) {
      setDevices([]);
      setError(null);
      setFastbootMissingFlag(false);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const list = await fastbootDevices();
      setDevices(list);
      setFastbootMissingFlag(false);
    } catch (e) {
      const msg = String(e);
      setError(msg);
      setDevices([]);
      if (isFastbootMissing(msg)) {
        setFastbootMissingFlag(true);
      } else {
        toast.error(msg);
      }
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void refresh();
    const id = setInterval(() => {
      void refresh();
    }, 5000);
    return () => clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [adbPath]);

  async function doReboot(serial: string, mode: 'recovery' | 'bootloader' | null) {
    const key = `${serial}:${mode ?? 'system'}`;
    setBusySerial(key);
    try {
      await fastbootReboot(serial, mode);
      const verb =
        mode === 'recovery' ? t('fastboot.rebootedRecovery', { serial })
        : mode === 'bootloader' ? t('fastboot.rebootedBootloader', { serial })
        : t('fastboot.rebooted', { serial });
      toast.success(verb);
      // After a reboot, the device will drop out of fastboot within
      // a second or two. Force one immediate refresh so the row
      // disappears promptly without waiting for the 5s poll.
      void refresh();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusySerial(null);
    }
  }

  async function toggleInfo(serial: string) {
    if (openInfoSerial === serial) {
      setOpenInfoSerial(null);
      return;
    }
    setOpenInfoSerial(serial);
    const existing = infoBySerial[serial];
    if (existing && existing.status !== 'error') return;
    setInfoBySerial((prev) => ({ ...prev, [serial]: { status: 'loading' } }));
    try {
      const data = await fastbootGetInfo(serial);
      setInfoBySerial((prev) => ({
        ...prev,
        [serial]: { status: 'loaded', data },
      }));
    } catch (e) {
      setInfoBySerial((prev) => ({
        ...prev,
        [serial]: { status: 'error', message: String(e) },
      }));
    }
  }

  async function retryInfo(serial: string) {
    setInfoBySerial((prev) => ({ ...prev, [serial]: { status: 'loading' } }));
    try {
      const data = await fastbootGetInfo(serial);
      setInfoBySerial((prev) => ({
        ...prev,
        [serial]: { status: 'loaded', data },
      }));
    } catch (e) {
      setInfoBySerial((prev) => ({
        ...prev,
        [serial]: { status: 'error', message: String(e) },
      }));
    }
  }

  async function ensureOemInfo(serial: string) {
    const existing = oemBySerial[serial];
    // Already loaded or definitively unsupported — no refetch.
    if (
      existing &&
      (existing.status === 'loaded' || existing.status === 'unsupported')
    ) {
      return;
    }
    setOemBySerial((prev) => ({ ...prev, [serial]: { status: 'loading' } }));
    try {
      const data = await fastbootGetOemDeviceInfo(serial);
      if (data.rawLines.length === 0) {
        setOemBySerial((prev) => ({
          ...prev,
          [serial]: { status: 'unsupported', message: 'empty' },
        }));
      } else {
        setOemBySerial((prev) => ({
          ...prev,
          [serial]: { status: 'loaded', rawLines: data.rawLines },
        }));
      }
    } catch (e) {
      setOemBySerial((prev) => ({
        ...prev,
        [serial]: { status: 'unsupported', message: String(e) },
      }));
    }
  }

  function retryOemInfo(serial: string) {
    // Force a refetch by clearing cached state then re-running the
    // ensure path. The "unsupported" state is sticky so the user
    // doesn't see the spinner flash on every toggle — Retry is the
    // explicit "I changed something, try again" affordance.
    setOemBySerial((prev) => {
      const next = { ...prev };
      delete next[serial];
      return next;
    });
    void ensureOemInfo(serial);
  }

  if (!adbPath) {
    return <FastbootNotConfiguredCard />;
  }

  if (fastbootMissingFlag) {
    return <FastbootMissingCard />;
  }

  return (
    <div className="space-y-4">
      <Card>
        <CardContent className="flex flex-wrap items-center gap-3">
          <div className="flex items-center gap-2 text-sm font-semibold text-text-0">
            <Zap className="h-4 w-4 text-brand" />
            {t('fastboot.connectionTitle')}
          </div>
          <div className="ml-2 flex items-center gap-2 text-xs text-text-2">
            {loading ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Badge variant="secondary">
                {t('fastboot.deviceCount', {
                  online: devices.length,
                  total: devices.length,
                })}
              </Badge>
            )}
          </div>
          <div className="ml-auto flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={() => void refresh()}
              disabled={loading}
            >
              <RefreshCw className={cn('h-4 w-4', loading && 'animate-spin')} />
              {t('fastboot.refresh')}
            </Button>
          </div>
        </CardContent>
      </Card>

      {error && !fastbootMissingFlag && (
        <Card className="border-danger">
          <CardContent className="flex items-start gap-3 text-sm">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-danger" />
            <div className="min-w-0 space-y-1">
              <div className="font-semibold text-text-0">
                {t('fastboot.errorTitle')}
              </div>
              <div className="break-all font-mono text-xs text-text-2">
                {error}
              </div>
            </div>
          </CardContent>
        </Card>
      )}

      <Card>
        <CardContent className="space-y-3">
          <div className="flex items-center gap-2 text-sm font-medium text-text-0">
            <Smartphone className="h-4 w-4 text-text-1" />
            {t('fastboot.localDevices')}
          </div>
          <p className="text-xs text-text-2">{t('fastboot.localHint')}</p>
          <div className="space-y-2">
            {devices.length === 0 ? (
              <div className="rounded-md border border-dashed border-border p-3 text-xs text-text-2">
                {loading ? t('fastboot.scanning') : t('fastboot.noDevices')}
              </div>
            ) : (
              devices.map((d) => (
                <FastbootDeviceRow
                  key={d.serial}
                  device={d}
                  busyKey={busySerial}
                  onReboot={doReboot}
                  infoState={infoBySerial[d.serial] ?? { status: 'idle' }}
                  infoOpen={openInfoSerial === d.serial}
                  onToggleInfo={() => void toggleInfo(d.serial)}
                  onRetryInfo={() => void retryInfo(d.serial)}
                  oemState={oemBySerial[d.serial] ?? { status: 'idle' }}
                  onFetchOem={() => void ensureOemInfo(d.serial)}
                  onRetryOem={() => retryOemInfo(d.serial)}
                />
              ))
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

/**
 * Per-row info panel. Renders the FastbootVarInfo as a 2-column grid
 * with the unlock-state variables (unlocked, secureboot) highlighted
 * via colored badges. Missing variables render as `fastboot.infoUnavailable`
 * (em-dash style) rather than `null`, since the device legitimately
 * omits them. On error we expose a Retry button that re-fetches without
 * closing the panel.
 */
function FastbootInfoPanel({
  state,
  onRetry,
  oemState,
  product,
  onFetchOem,
  onRetryOem,
}: {
  state: Extract<InfoStateUnion, { status: 'loaded' | 'loading' | 'error' }> | { status: 'idle' };
  onRetry: () => void;
  oemState: OemSectionState;
  product: string | null;
  onFetchOem: () => void;
  onRetryOem: () => void;
}) {
  const { t } = useTranslation();
  // Trigger the OEM diagnostics fetch as soon as the var payload is
  // available. We don't gate on panel open state — the parent already
  // keeps cached payload around, and prefetching while the vars load
  // gives the OEM section a head start so both finish around the
  // same time.
  useEffect(() => {
    if (state.status === 'loaded' && oemState.status === 'idle') {
      onFetchOem();
    }
  }, [state.status, oemState.status, onFetchOem]);
  if (state.status === 'idle') return null;
  if (state.status === 'loading') {
    return (
      <div className="flex items-center gap-2 px-3 py-2 text-xs text-text-2">
        <Loader2 className="h-3 w-3 animate-spin" />
        {t('fastboot.infoLoading')}
      </div>
    );
  }
  if (state.status === 'error') {
    return (
      <div className="flex items-center justify-between gap-2 px-3 py-2 text-xs">
        <div className="flex items-center gap-2 text-danger">
          <AlertTriangle className="h-3 w-3" />
          <span>{t('fastboot.infoError')}</span>
          <span className="font-mono text-text-2">· {state.message}</span>
        </div>
        <Button size="sm" variant="outline" onClick={onRetry}>
          <RefreshCw className="h-3 w-3" />
          {t('fastboot.infoRetry')}
        </Button>
      </div>
    );
  }
  const d = state.data;
  const na = t('fastboot.infoUnavailable');
  const yesLabel = t('fastboot.varValueYes');
  const noLabel = t('fastboot.varValueNo');

  // Render a boolean variable as a green/red badge. Accepts the
  // canonical yes/no plus numeric 0/1 and literal true/false — Xiaomi
  // bootloaders, in particular, report booleans as bare 0/1 strings,
  // so missing the alternate spellings would leave the cell rendered
  // as a plain `0` that the user has to mentally convert.
  const renderBoolBadge = (raw: string | null, successIsYes = true) => {
    if (raw === null) return <PlainValue>{na}</PlainValue>;
    const lower = raw.toLowerCase().trim();
    const isYes = lower === 'yes' || lower === '1' || lower === 'true';
    const isNo = lower === 'no' || lower === '0' || lower === 'false';
    if (!isYes && !isNo) return <PlainValue>{raw}</PlainValue>;
    // For some vars (e.g. off-mode-charge) `no` is the *bad* value;
    // for others (e.g. secureboot) `no` was the good value. The
    // caller decides which polarity is success.
    const isSuccess = successIsYes ? isYes : isNo;
    return (
      <span
        className={cn(
          'inline-flex items-center gap-1 rounded-md px-2 py-0.5 text-xs font-semibold',
          isSuccess ? 'bg-success/15 text-success' : 'bg-danger/15 text-danger',
        )}
      >
        {isYes ? <Check className="h-3 w-3" /> : <X className="h-3 w-3" />}
        {isYes ? yesLabel : noLabel}
      </span>
    );
  };

  // Render Verified Boot state. The four values mean:
  //   green  — verified, OEM keys trusted, no warnings
  //   yellow — user-built OS booted but still verified by custom key
  //   orange — unlocked bootloader, no signing enforced
  //   red    — verification failed (refuse to boot)
  const renderVerifiedBootBadge = (raw: string | null) => {
    if (raw === null) return <PlainValue>{na}</PlainValue>;
    const state = raw.toLowerCase();
    const cls =
      state === 'green' ? 'bg-success/15 text-success'
      : state === 'yellow' ? 'bg-warning/15 text-warning'
      : state === 'orange' ? 'bg-warning/25 text-warning-strong'
      : state === 'red' ? 'bg-danger/15 text-danger'
      : null;
    const labelKey =
      state === 'green' ? 'fastboot.vbStateGreen'
      : state === 'yellow' ? 'fastboot.vbStateYellow'
      : state === 'orange' ? 'fastboot.vbStateOrange'
      : state === 'red' ? 'fastboot.vbStateRed'
      : null;
    if (!cls || !labelKey) return <PlainValue>{raw}</PlainValue>;
    return (
      <span
        className={cn(
          'inline-flex items-center gap-1 rounded-md px-2 py-0.5 text-xs font-semibold',
          cls,
        )}
      >
        {t(labelKey)}
      </span>
    );
  };

  // Render max-download-size with a friendly GB/MB suffix. Bootloaders
  // split roughly half-and-half between two spellings: Pixel / AOSP
  // ship the value as a `0x`-prefixed hex string, Xiaomi / Qualcomm
  // ship it as a bare decimal. We accept both and fall back to the raw
  // text if neither parses, so a misformatted value doesn't crash the
  // cell — it just shows the literal bootloader output.
  const renderDownloadSize = (raw: string | null) => {
    if (raw === null) return <PlainValue>{na}</PlainValue>;
    const trimmed = raw.trim();
    let bytes = 0;
    // TS can't prove the capture group exists when match() returns
    // truthy — the regex requires `+` so the group is always present
    // on a match, but the type system disagrees. Narrow explicitly.
    const hexMatch = trimmed.match(/^0x([0-9a-f]+)$/i);
    if (hexMatch && hexMatch[1]) {
      bytes = parseInt(hexMatch[1], 16);
    } else if (/^-?\d+$/.test(trimmed)) {
      bytes = parseInt(trimmed, 10);
    }
    if (!Number.isFinite(bytes) || bytes <= 0) return <PlainValue>{raw}</PlainValue>;
    const units: Array<[number, string]> = [
      [1024 ** 3, 'GB'],
      [1024 ** 2, 'MB'],
      [1024, 'KB'],
    ];
    let human = '';
    for (const [size, unit] of units) {
      if (bytes >= size) {
        human = `· ${(bytes / size).toFixed(2)} ${unit}`;
        break;
      }
    }
    if (!human) human = `· ${bytes} B`;
    return (
      <span className="font-mono text-text-0">
        {raw} <span className="text-text-2">{human}</span>
      </span>
    );
  };

  // Render an unknown / monospace value as plain mono text.
  const renderPlain = (raw: string | null) =>
    raw === null ? <PlainValue>{na}</PlainValue> : <PlainValue>{raw}</PlainValue>;

  // Field order here drives the visual order of the grid; the groupings
  // (security → versions → flashing → slots) make the panel scannable
  // when half the cells are empty on devices that don't report a var.
  const items: InfoCell[] = [
    { key: 'unlocked', label: t('fastboot.varUnlocked'), node: renderBoolBadge(d.unlocked, /*successIsYes=*/ true) },
    { key: 'verifiedBoot', label: t('fastboot.varVerifiedBootState'), node: renderVerifiedBootBadge(d.verifiedBootState) },
    { key: 'hardware', label: t('fastboot.varHardware'), node: renderPlain(d.hardware) },
    { key: 'variant', label: t('fastboot.varVariant'), node: renderPlain(d.variant) },
    { key: 'verBl', label: t('fastboot.varVersionBootloader'), node: renderPlain(d.versionBootloader) },
    { key: 'verHw', label: t('fastboot.varVersionHardware'), node: renderPlain(d.versionHardware) },
    { key: 'verBb', label: t('fastboot.varVersionBaseband'), node: renderPlain(d.versionBaseband) },
    { key: 'product', label: t('fastboot.varProduct'), node: renderPlain(d.product) },
    { key: 'maxDl', label: t('fastboot.varMaxDownloadSize'), node: renderDownloadSize(d.maxDownloadSize) },
    { key: 'offCharge', label: t('fastboot.varOffModeCharge'), node: renderBoolBadge(d.offModeCharge, /*successIsYes=*/ true) },
    { key: 'batt', label: t('fastboot.varBatterySocOk'), node: renderBoolBadge(d.batterySocOk, /*successIsYes=*/ true) },
    { key: 'antiRb', label: t('fastboot.varAntiRollback'), node: renderPlain(d.antiRollback) },
    { key: 'curSlot', label: t('fastboot.varCurrentSlot'), node: renderPlain(d.currentSlot) },
    { key: 'slotCnt', label: t('fastboot.varSlotCount'), node: renderPlain(d.slotCount) },
    { key: 'serial', label: t('fastboot.varSerialno'), node: renderPlain(d.serialno) },
  ];
  // The OEM section needs `product` for vendor detection, so only
  // render it once the var payload has loaded. While vars are still
  // loading the section would have nothing useful to display anyway.
  // (`product` is also passed as a prop for compatibility with devices
  // that omit it from the var payload — in practice var loading and
  // product availability always coincide, but the prop keeps the
  // contract clean.)
  return (
    <div className="px-3 pb-3 pt-1">
      <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
        {items.map((it) => (
          <div
            key={it.key}
            className="rounded-md border border-border bg-bg-0 px-3 py-2"
          >
            <div className="text-[10px] font-medium uppercase tracking-wide text-text-2">
              {it.label}
            </div>
            <div className="mt-1 text-sm">{it.node}</div>
          </div>
        ))}
      </div>
      {state.status === 'loaded' && (
        <OemInfoSection
          state={oemState}
          product={product}
          onRetry={onRetryOem}
        />
      )}
    </div>
  );
}


/**
 * One cell in the FastbootInfoPanel grid. `node` is pre-rendered by the
 * parent so per-cell styling (badge vs plain mono vs hex+GB) doesn't
 * have to live inside the map().
 */
type InfoCell = { key: string; label: string; node: React.ReactNode };

function PlainValue({ children }: { children: React.ReactNode }) {
  return <span className="font-mono text-text-0">{children}</span>;
}

// Mirror the InfoState union so FastbootInfoPanel's prop type stays in
// sync with the parent without an import cycle.
type InfoStateUnion =
  | { status: 'idle' }
  | { status: 'loading' }
  | { status: 'loaded'; data: FastbootVarInfo }
  | { status: 'error'; message: string };

function FastbootDeviceRow({
  device,
  busyKey,
  onReboot,
  infoState,
  infoOpen,
  onToggleInfo,
  onRetryInfo,
  oemState,
  onFetchOem,
  onRetryOem,
}: {
  device: FastbootDevice;
  busyKey: string | null;
  onReboot: (serial: string, mode: 'recovery' | 'bootloader' | null) => void;
  infoState: InfoStateUnion;
  infoOpen: boolean;
  onToggleInfo: () => void;
  onRetryInfo: () => void;
  oemState: OemSectionState;
  onFetchOem: () => void;
  onRetryOem: () => void;
}) {
  const { t } = useTranslation();
  const anyBusy = busyKey !== null;
  const infoLoading = infoState.status === 'loading';
  return (
    <div
      className={cn(
        'w-full rounded-md border bg-bg-1 text-sm',
        'border-border',
      )}
    >
      <div className="flex w-full flex-wrap items-center justify-between gap-3 px-3 py-2">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <Terminal className="h-4 w-4 shrink-0 text-brand" />
            <span className="truncate font-mono text-xs text-text-0">
              {device.serial}
            </span>
            <Badge variant="success">{device.state}</Badge>
          </div>
          <div className="mt-0.5 truncate text-xs text-text-2">
            {device.model ?? device.product ?? t('fastboot.unknownDevice')}
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            size="sm"
            variant={infoOpen ? 'default' : 'outline'}
            disabled={anyBusy && infoLoading}
            onClick={onToggleInfo}
          >
            {infoLoading ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Info className="h-3 w-3" />
            )}
            {infoOpen ? t('fastboot.hideInfo') : t('fastboot.getInfo')}
            <ChevronDown
              className={cn(
                'h-3 w-3 transition-transform',
                infoOpen && 'rotate-180',
              )}
            />
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={anyBusy}
            onClick={() => onReboot(device.serial, null)}
          >
            {busyKey === `${device.serial}:system` ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <RotateCcw className="h-3 w-3" />
            )}
            {t('fastboot.reboot')}
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={anyBusy}
            onClick={() => onReboot(device.serial, 'recovery')}
          >
            {busyKey === `${device.serial}:recovery` ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <RotateCcw className="h-3 w-3" />
            )}
            {t('fastboot.rebootRecovery')}
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={anyBusy}
            onClick={() => onReboot(device.serial, 'bootloader')}
          >
            {busyKey === `${device.serial}:bootloader` ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <RotateCcw className="h-3 w-3" />
            )}
            {t('fastboot.rebootBootloader')}
          </Button>
        </div>
      </div>
      {infoOpen && (
        <div className="border-t border-border bg-bg-0/40">
          <FastbootInfoPanel
            state={infoState}
            onRetry={onRetryInfo}
            oemState={oemState}
            product={device.product}
            onFetchOem={onFetchOem}
            onRetryOem={onRetryOem}
          />
        </div>
      )}
    </div>
  );
}

/**
 * Empty-state shown when `settings.adb_path` is unset. Same UX as
 * `AdbConnectionPanel`'s not-configured card — deep-link to Settings
 * → Tools.
 */
function FastbootNotConfiguredCard() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  return (
    <Card className="border-warning">
      <CardContent className="flex flex-wrap items-start gap-4 text-sm">
        <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-warning" />
        <div className="min-w-0 flex-1 space-y-2">
          <div className="font-semibold text-text-0">
            {t('fastboot.fastbootMissingTitle')}
          </div>
          <p className="text-text-2">{t('fastboot.fastbootMissingDesc')}</p>
        </div>
        <Button
          onClick={() => navigate('/settings?tab=tools')}
          className="shrink-0"
        >
          {t('fastboot.fastbootMissingCta')}
        </Button>
      </CardContent>
    </Card>
  );
}

/**
 * Banner shown when adb is configured but the fastboot binary is not
 * next to it. Same shape as the not-configured card so the user has a
 * single mental model for "tool missing → go to settings".
 */
function FastbootMissingCard() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  return (
    <Card className="border-warning">
      <CardContent className="flex flex-wrap items-start gap-4 text-sm">
        <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-warning" />
        <div className="min-w-0 flex-1 space-y-2">
          <div className="font-semibold text-text-0">
            {t('fastboot.fastbootMissingTitle')}
          </div>
          <p className="text-text-2">{t('fastboot.fastbootMissingDesc')}</p>
        </div>
        <Button
          onClick={() => navigate('/settings?tab=tools')}
          className="shrink-0"
        >
          {t('fastboot.fastbootMissingCta')}
        </Button>
      </CardContent>
    </Card>
  );
}
