import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { toast } from 'sonner';
import {
  AlertTriangle,
  FileUp,
  Loader2,
  RefreshCw,
  RotateCcw,
  ShieldAlert,
  Smartphone,
  Terminal,
  Zap,
} from 'lucide-react';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  adbDevices,
  adbReboot,
  adbRecoveryInfo,
  adbSideload,
  type AdbDevice,
  type RecoveryInfo,
  type RecoveryType,
} from '@/ipc/adb';
import { useSettingsStore } from '@/store/settings';
import { cn } from '@/lib/utils';

/** States a recovery-mode device can be in according to `adb devices`.
 *  We only render devices in one of these two states; everything else
 *  (offline / unauthorized / device) is filtered out as "not in
 *  recovery mode right now". */
const RECOVERY_STATES = new Set(['recovery', 'sideload']);

/**
 * Recovery tab content. Mirrors the layout conventions of the
 * FastbootPanel — header card with refresh + count, body card
 * listing devices, 5 s polling so a freshly-entered recovery device
 * shows up without a manual reload. Each device row exposes a
 * "Sideload update" action (file picker → `adb sideload`) plus the
 * two safe recovery-side reboots.
 */
export function RecoveryPanel() {
  const { t } = useTranslation();
  const adbPath = useSettingsStore((s) => s.settings?.adbPath ?? null);
  const [allDevices, setAllDevices] = useState<AdbDevice[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    if (!adbPath) {
      setAllDevices([]);
      setError(null);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const list = await adbDevices();
      setAllDevices(list);
    } catch (e) {
      const msg = String(e);
      setError(msg);
      setAllDevices([]);
      toast.error(msg);
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

  if (!adbPath) {
    return (
      <RecoveryEmptyCard
        title={t('recovery.title')}
        message={t('adb.adbMissingDesc')}
        cta={t('adb.adbMissingCta')}
        onCta={() => { /* deep-link handled by adb panel */ }}
      />
    );
  }

  const recoveryDevices = allDevices.filter((d) => RECOVERY_STATES.has(d.state));

  return (
    <div className="space-y-4">
      <Card>
        <CardContent className="flex flex-wrap items-center gap-3">
          <div className="flex items-center gap-2 text-sm font-semibold text-text-0">
            <ShieldAlert className="h-4 w-4 text-brand" />
            {t('recovery.connectionTitle')}
          </div>
          <div className="ml-2 flex items-center gap-2 text-xs text-text-2">
            {loading ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Badge variant="secondary">
                {t('recovery.deviceCount', { count: recoveryDevices.length })}
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
              {t('recovery.refresh')}
            </Button>
          </div>
        </CardContent>
      </Card>

      {error && (
        <Card className="border-danger">
          <CardContent className="flex items-start gap-3 text-sm">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-danger" />
            <div className="min-w-0 space-y-1">
              <div className="font-semibold text-text-0">
                {t('recovery.infoError')}
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
            {t('recovery.connectionTitle')}
          </div>
          <p className="text-xs text-text-2">{t('recovery.localHint')}</p>
          <div className="space-y-2">
            {recoveryDevices.length === 0 ? (
              <div className="rounded-md border border-dashed border-border p-3 text-xs text-text-2">
                {loading ? t('recovery.scanning') : t('recovery.noRecoveryDevices')}
              </div>
            ) : (
              recoveryDevices.map((d) => (
                <RecoveryDeviceRow key={d.serial} device={d} />
              ))
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

/**
 * Per-device row. Owns:
 *   - Recovery-info state machine (idle / loading / loaded / error)
 *   - Per-device sideload + reboot busy flags
 * Recovery info is fetched unconditionally on mount — unlike the
 * fastboot panel where the var grid is hidden behind a toggle, the
 * Recovery tab is purpose-built around this info.
 */
function RecoveryDeviceRow({ device }: { device: AdbDevice }) {
  const { t } = useTranslation();
  const [infoState, setInfoState] = useState<
    | { status: 'idle' }
    | { status: 'loading' }
    | { status: 'loaded'; data: RecoveryInfo }
    | { status: 'error'; message: string }
  >({ status: 'idle' });
  const [sideloadBusy, setSideloadBusy] = useState(false);
  const [rebootBusy, setRebootBusy] = useState<null | 'system' | 'bootloader'>(null);

  useEffect(() => {
    let cancelled = false;
    setInfoState({ status: 'loading' });
    adbRecoveryInfo(device.serial)
      .then((data) => {
        if (!cancelled) setInfoState({ status: 'loaded', data });
      })
      .catch((e) => {
        if (!cancelled) setInfoState({ status: 'error', message: String(e) });
      });
    return () => {
      cancelled = true;
    };
  }, [device.serial]);

  async function doReboot(mode: 'recovery' | 'bootloader' | null) {
    setRebootBusy(mode === null ? 'system' : 'bootloader');
    try {
      await adbReboot(device.serial, mode);
      // After reboot the device leaves recovery state; the parent
      // poll will drop this row from the list within 5 s. We don't
      // manually optimistically hide it because the user benefits
      // from seeing the toast confirm the action went through.
    } catch (e) {
      toast.error(String(e));
    } finally {
      setRebootBusy(null);
    }
  }

  async function doSideload() {
    if (sideloadBusy) return;
    if (device.state === 'sideload') {
      toast.error(t('recovery.applying') + '…');
      return;
    }
    const picked = await openDialog({
      multiple: false,
      filters: [{ name: 'OTA / APK', extensions: ['zip', 'apk'] }],
    });
    if (!picked || Array.isArray(picked)) return;
    const path = String(picked);
    setSideloadBusy(true);
    try {
      const out = await adbSideload(device.serial, path);
      const ok = /total xfer/i.test(out);
      const filename = path.split(/[\\/]/).pop() ?? path;
      if (ok) {
        toast.success(t('recovery.applySuccess', { path: filename }));
      } else {
        toast.error(t('recovery.applyFailed', { error: out || 'unknown' }));
      }
    } catch (e) {
      const msg = String(e);
      toast.error(t('recovery.applyFailed', { error: msg }));
    } finally {
      setSideloadBusy(false);
    }
  }

  return (
    <div className="rounded-md border border-border bg-bg-1 px-3 py-2 text-sm">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <Terminal className="h-4 w-4 shrink-0 text-brand" />
            <span className="truncate font-mono text-xs text-text-0">
              {device.serial}
            </span>
            <RecoveryStateBadge state={device.state} />
          </div>
          <RecoveryInfoLine state={infoState} />
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            size="sm"
            variant="default"
            disabled={sideloadBusy || device.state === 'sideload'}
            onClick={() => void doSideload()}
            title={t('recovery.applyHint')}
          >
            {sideloadBusy ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <FileUp className="h-3 w-3" />
            )}
            {t('recovery.applyOta')}
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={rebootBusy !== null}
            onClick={() => void doReboot(null)}
          >
            {rebootBusy === 'system' ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <RotateCcw className="h-3 w-3" />
            )}
            {t('recovery.rebootSystem')}
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={rebootBusy !== null}
            onClick={() => void doReboot('bootloader')}
          >
            {rebootBusy === 'bootloader' ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Zap className="h-3 w-3" />
            )}
            {t('recovery.rebootBootloader')}
          </Button>
        </div>
      </div>
    </div>
  );
}

function RecoveryStateBadge({ state }: { state: string }) {
  const { t } = useTranslation();
  if (state === 'sideload') {
    return (
      <span className="inline-flex items-center gap-1 rounded-md bg-warning/15 px-2 py-0.5 text-xs font-semibold text-warning">
        <Zap className="h-3 w-3" />
        {t('recovery.stateSideload')}
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1 rounded-md bg-brand/15 px-2 py-0.5 text-xs font-semibold text-brand">
      <ShieldAlert className="h-3 w-3" />
      {t('recovery.stateRecovery')}
    </span>
  );
}

function RecoveryInfoLine({
  state,
}: {
  state:
    | { status: 'idle' }
    | { status: 'loading' }
    | { status: 'loaded'; data: RecoveryInfo }
    | { status: 'error'; message: string };
}) {
  const { t } = useTranslation();
  if (state.status === 'idle') return null;
  if (state.status === 'loading') {
    return (
      <div className="mt-1 flex items-center gap-2 text-xs text-text-2">
        <Loader2 className="h-3 w-3 animate-spin" />
        {t('recovery.infoHint')}
      </div>
    );
  }
  if (state.status === 'error') {
    return (
      <div className="mt-1 truncate text-xs text-danger">
        {t('recovery.infoError')} · <span className="font-mono text-text-2">{state.message}</span>
      </div>
    );
  }
  const d = state.data;
  const na = t('recovery.infoUnavailable');
  // Brand hint: prefer `brand` when it's distinct from `manufacturer`
  // (e.g. Redmi vs Xiaomi), otherwise just show the manufacturer.
  // Falls back to neither if both probes failed — we don't render an
  // empty badge just to fill the row.
  const brandHint =
    d.manufacturer && d.brand && d.brand.toLowerCase() !== d.manufacturer.toLowerCase()
      ? `${d.manufacturer} · ${d.brand}`
      : d.manufacturer;
  return (
    <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
      <RecoveryTypeBadge type={d.recoveryType} />
      {brandHint && (
        <span className="inline-flex items-center rounded-md border border-border bg-bg-2 px-2 py-0.5 font-medium text-text-1">
          {brandHint}
        </span>
      )}
      {d.version && (
        <span className="font-mono text-text-1">{d.version}</span>
      )}
      {d.model && (
        <>
          <span className="text-text-2">·</span>
          <span className="text-text-2">{d.model}</span>
        </>
      )}
      {d.buildFingerprint && (
        <>
          <span className="text-text-2">·</span>
          <span className="truncate font-mono text-text-2">{d.buildFingerprint}</span>
        </>
      )}
      {!brandHint && !d.version && !d.model && !d.buildFingerprint && (
        <span className="text-text-2">{na}</span>
      )}
    </div>
  );
}

function RecoveryTypeBadge({ type }: { type: RecoveryType }) {
  const { t } = useTranslation();
  const key =
    type === 'twrp' ? 'typeTwrp'
    : type === 'orangefox' ? 'typeOrangeFox'
    : type === 'lineageos' ? 'typeLineageOs'
    : type === 'aosp' ? 'typeAosp'
    : type === 'stock' ? 'typeStock'
    : 'typeUnknown';
  // Custom recoveries get a more colorful badge; stock / unknown get
  // a muted one — visually signals "your device is in a known-good
  // custom recovery" vs "we have no idea what's running".
  const isCustom = type === 'twrp' || type === 'orangefox' || type === 'lineageos';
  const cls = isCustom
    ? 'bg-brand/15 text-brand'
    : type === 'aosp'
    ? 'bg-success/15 text-success'
    : 'bg-bg-2 text-text-1 border border-border';
  return (
    <span className={cn('inline-flex items-center gap-1 rounded-md px-2 py-0.5 text-xs font-semibold', cls)}>
      {t(`recovery.${key}`)}
    </span>
  );
}

/**
 * Placeholder card when adb isn't configured. We deliberately don't
 * deep-link to settings here — the ADB tab already shows the same
 * CTA, and we'd be tracking two settings-entry points that could
 * drift. Keep this card purely informational.
 */
function RecoveryEmptyCard({
  title,
  message,
  cta,
  onCta,
}: {
  title: string;
  message: string;
  cta: string;
  onCta: () => void;
}) {
  return (
    <Card className="border-warning">
      <CardContent className="flex flex-wrap items-start gap-4 text-sm">
        <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-warning" />
        <div className="min-w-0 flex-1 space-y-2">
          <div className="font-semibold text-text-0">{title}</div>
          <p className="text-text-2">{message}</p>
        </div>
        <Button onClick={onCta} className="shrink-0">{cta}</Button>
      </CardContent>
    </Card>
  );
}
