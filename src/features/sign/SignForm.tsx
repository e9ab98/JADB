import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useLicenseStore } from '@/store/license';
import '@/i18n';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Card, CardContent } from '@/components/ui/card';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  checkApkSigned,
  signApk,
  pickApk,
  pickStripOutputDir,
  inspectSignature,
  stripApkSigning,
  type SignRequest,
} from '@/ipc/sign';
import type { TaskHandle } from '@/ipc/types';
import { listSignatures, type SignatureConfig } from '@/ipc/signatures';
import { listLineages, type LineageStatus } from '@/ipc/lineages';
import { SigningSchemeSelector } from '@/features/sign/SigningSchemeSelector';
import { useProgress } from '@/hooks/useProgress';
import {
  DEFAULT_SIGNING_SCHEMES,
  type SignatureInfo,
  type SigningSchemes,
  type StripResult,
} from '@/types/signing';
import { ROTATION_MIN_SDK_VERSION } from '@/types/lineage';

type Props = { onStarted: (h: TaskHandle) => void };

type StandardRequest = {
  kind: 'standard';
  apkPath: string;
  signatureId: string;
  signatureLabel: string;
  schemes: SigningSchemes;
};

type RotationRequest = {
  kind: 'rotation';
  apkPath: string;
  lineageId: string;
  lineageLabel: string;
  oldSignatureLabel: string;
  newSignatureLabel: string;
  v4Enabled: boolean;
};

type PendingSign = StandardRequest | RotationRequest;

type SchemeRisk = { kind: 'error'; key: string } | { kind: 'confirm'; key: string };

function detectSchemeRisk(s: SigningSchemes): SchemeRisk | null {
  if (s.v4 && !s.v2 && !s.v3) {
    return { kind: 'error', key: 'riskV4NeedsV2OrV3' };
  }
  if (!s.v1 && (s.v2 || s.v3 || s.v4)) {
    return { kind: 'confirm', key: 'riskNoV1' };
  }
  if (s.v3 && !s.v2) {
    return { kind: 'confirm', key: 'riskV3NoV2' };
  }
  return null;
}

export function SignForm({ onStarted }: Props) {
  const { t } = useTranslation();
  const [apk, setApk] = useState<string | null>(null);
  const [sigs, setSigs] = useState<SignatureConfig[]>([]);
  const [lineages, setLineages] = useState<LineageStatus[]>([]);
  const [busy, setBusy] = useState(false);
  const [checking, setChecking] = useState(false);
  const [pendingResign, setPendingResign] = useState<PendingSign | null>(null);
  const [pendingRisk, setPendingRisk] = useState<PendingSign | null>(null);
  const [riskKey, setRiskKey] = useState<string | null>(null);
  const [mode, setMode] = useState<'standard' | 'rotation'>('standard');

  // Standard mode state (preserved across tab switches).
  const [sigId, setSigId] = useState<string | null>(null);
  const [schemes, setSchemes] = useState<SigningSchemes>({ ...DEFAULT_SIGNING_SCHEMES });

  // Rotation mode state (preserved across tab switches).
  const [lineageId, setLineageId] = useState<string | null>(null);
  const [rotationV4, setRotationV4] = useState(true);

  const [inspecting, setInspecting] = useState(false);
  const [signatureInfo, setSignatureInfo] = useState<SignatureInfo | null>(null);

  const [pendingStrip, setPendingStrip] = useState<string | null>(null);
  const [stripTaskId, setStripTaskId] = useState<string | null>(null);
  const stripTask = useProgress(stripTaskId);
  const [stripResult, setStripResult] = useState<StripResult | null>(null);
  const stripping = stripTask?.status === 'running';

  useEffect(() => {
    void listSignatures()
      .then(setSigs)
      .catch((e) => toast.error(String(e)));
  }, []);

  useEffect(() => {
    void listLineages()
      .then(setLineages)
      .catch((e) => toast.error(String(e)));
  }, []);

  useEffect(() => {
    if (!stripTask) return;
    if (stripTask.status === 'done' && stripTask.result) {
      const result = stripTask.result as StripResult;
      setStripResult(result);
      setApk(result.outputPath);
      setStripTaskId(null);
    } else if (stripTask.status === 'error') {
      toast.error(stripTask.error ?? 'strip failed');
      setStripTaskId(null);
    } else if (stripTask.status === 'cancelled') {
      setStripTaskId(null);
    }
  }, [stripTask?.status, stripTask?.result, stripTask?.error]);

  async function dispatch(request: SignRequest) {
    const handle = await signApk(request);
    onStarted(handle);
    toast.success(t('sign.taskStarted'));
  }

  async function runStandard(request: StandardRequest, allowResign: boolean) {
    await dispatch({
      mode: 'standard',
      apkPath: request.apkPath,
      signatureId: request.signatureId,
      allowResign,
      schemes: request.schemes,
    });
  }

  async function runRotation(request: RotationRequest, allowResign: boolean) {
    await dispatch({
      mode: 'rotation',
      apkPath: request.apkPath,
      lineageId: request.lineageId,
      allowResign,
      v4Enabled: request.v4Enabled,
    });
  }

  async function startStandard() {
    if (busy || !apk || !sigId) return;
    const signature = sigs.find((item) => item.id === sigId);
    const request: StandardRequest = {
      kind: 'standard',
      apkPath: apk,
      signatureId: sigId,
      signatureLabel: signature?.label || sigId,
      schemes: { ...schemes },
    };
    setBusy(true);
    setChecking(true);
    try {
      const signed = await checkApkSigned(request.apkPath);
      setChecking(false);
      if (signed) {
        setPendingResign(request);
        return;
      }
      const risk = detectSchemeRisk(request.schemes);
      if (risk) {
        if (risk.kind === 'error') {
          toast.error(t(`sign.${risk.key}`));
          return;
        }
        setPendingRisk(request);
        setRiskKey(risk.key);
        return;
      }
      await runStandard(request, false);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
      setChecking(false);
    }
  }

  async function startRotation() {
    if (busy || !apk || !lineageId) return;
    if (!useLicenseStore.getState().requireFeature('signing_v31')) return;
    const entry = lineages.find((item) => item.config.id === lineageId);
    if (!entry) {
      toast.error(t('sign.rotationMissingLineage'));
      return;
    }
    const oldSig = sigs.find((s) => s.id === entry.config.oldSignatureId);
    const newSig = sigs.find((s) => s.id === entry.config.newSignatureId);
    const request: RotationRequest = {
      kind: 'rotation',
      apkPath: apk,
      lineageId,
      lineageLabel: entry.config.label,
      oldSignatureLabel: oldSig?.label ?? entry.config.oldSignatureId,
      newSignatureLabel: newSig?.label ?? entry.config.newSignatureId,
      v4Enabled: rotationV4,
    };
    setBusy(true);
    setChecking(true);
    try {
      const signed = await checkApkSigned(request.apkPath);
      setChecking(false);
      if (signed) {
        setPendingResign(request);
        return;
      }
      await runRotation(request, false);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
      setChecking(false);
    }
  }

  async function confirmResign() {
    if (busy || !pendingResign) return;
    setBusy(true);
    try {
      if (pendingResign.kind === 'standard') {
        await runStandard(pendingResign, true);
      } else {
        await runRotation(pendingResign, true);
      }
      setPendingResign(null);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function confirmRisk() {
    if (busy || !pendingRisk) return;
    // runStandard() requires a StandardRequest (not RotationRequest);
    // rotation requests have their own path and never reach confirmRisk,
    // but TS can't see that so we narrow explicitly here.
    if (pendingRisk.kind !== 'standard') return;
    setBusy(true);
    try {
      await runStandard(pendingRisk, false);
      setPendingRisk(null);
      setRiskKey(null);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  }

  function cancelRisk() {
    if (busy) return;
    setPendingRisk(null);
    setRiskKey(null);
  }

  async function onInspect() {
    if (!apk || inspecting) return;
    setInspecting(true);
    setSignatureInfo(null);
    try {
      const info = await inspectSignature(apk);
      setSignatureInfo(info);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setInspecting(false);
    }
  }

  function closeInspect() {
    if (inspecting) return;
    setSignatureInfo(null);
  }

  function onStripClick() {
    if (!apk || stripping) return;
    setPendingStrip(apk);
  }

  function cancelStrip() {
    if (stripping) return;
    setPendingStrip(null);
  }

  async function confirmStrip() {
    const source = pendingStrip;
    if (!source || stripping) return;
    let dir: string | null;
    try {
      dir = await pickStripOutputDir();
    } catch (e) {
      toast.error(String(e));
      return;
    }
    if (!dir) {
      setPendingStrip(null);
      return;
    }
    const sep = dir.includes('\\') ? '\\' : '/';
    const filename = defaultStrippedName(source);
    const output = dir.endsWith(sep) ? `${dir}${filename}` : `${dir}${sep}${filename}`;
    setPendingStrip(null);
    try {
      const handle = await stripApkSigning(source, output);
      setStripTaskId(handle.task_id);
      onStarted(handle);
    } catch (e) {
      toast.error(String(e));
    }
  }

  function closeStripResult() {
    setStripResult(null);
  }

  const availableLineages = lineages.filter(
    (l) => l.config.oldSignatureId && l.config.newSignatureId,
  );
  const selectedLineage =
    availableLineages.find((l) => l.config.id === lineageId) ?? null;
  const lineageUsable =
    !!selectedLineage &&
    selectedLineage.fileExists &&
    selectedLineage.oldSignatureExists &&
    selectedLineage.newSignatureExists;

  return (
    <>
      <Card>
        <CardContent className="grid gap-3">
          <div className="flex gap-2">
            <Input readOnly value={apk ?? ''} placeholder={t('sign.apkPlaceholder')} />
            <Button
              variant="outline"
              disabled={busy || stripping}
              onClick={async () => setApk(await pickApk())}
            >
              {t('common.open')}
            </Button>
          </div>

          <div className="flex flex-wrap gap-2">
            <Button
              variant="outline"
              disabled={!apk || busy || inspecting || stripping}
              onClick={() => void onInspect()}
            >
              {inspecting ? t('sign.inspecting') : t('sign.inspect')}
            </Button>
            <Button
              variant="outline"
              disabled={!apk || busy || stripping || inspecting}
              onClick={onStripClick}
            >
              {stripping ? t('sign.stripping') : t('sign.strip')}
            </Button>
          </div>

          <Tabs
            value={mode}
            onValueChange={(value) => {
              if (value === 'standard') setMode(value);
              if (value === 'rotation' && useLicenseStore.getState().requireFeature('signing_v31')) {
                setMode(value);
              }
            }}
          >
            <TabsList>
              <TabsTrigger value="standard">{t('sign.modeStandard')}</TabsTrigger>
              <TabsTrigger value="rotation">{t('sign.modeRotation')} · VIP</TabsTrigger>
            </TabsList>

            <TabsContent value="standard" className="grid gap-3">
              <select
                value={sigId ?? ''}
                onChange={(e) => setSigId(e.target.value || null)}
                disabled={busy || stripping}
                className="h-9 rounded-md border border-border bg-bg-1 px-2 text-sm text-text-0 disabled:opacity-50"
              >
                <option value="">{t('sign.selectSignature')}</option>
                {sigs.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.label}
                  </option>
                ))}
              </select>
              <SigningSchemeSelector
                value={schemes}
                onChange={setSchemes}
                disabled={busy || stripping}
              />
              <div className="flex flex-wrap gap-2">
                <Button
                  disabled={busy || !apk || !sigId}
                  onClick={() => void startStandard()}
                >
                  {busy
                    ? t(checking ? 'sign.checking' : 'sign.starting')
                    : t('sign.start')}
                </Button>
              </div>
            </TabsContent>

            <TabsContent value="rotation" className="grid gap-3">
              <select
                value={lineageId ?? ''}
                onChange={(e) => setLineageId(e.target.value || null)}
                disabled={busy || stripping}
                className="h-9 rounded-md border border-border bg-bg-1 px-2 text-sm text-text-0 disabled:opacity-50"
              >
                <option value="">{t('sign.selectLineage')}</option>
                {availableLineages.map((l) => {
                  const oldLabel =
                    sigs.find((s) => s.id === l.config.oldSignatureId)?.label ??
                    l.config.oldSignatureId;
                  const newLabel =
                    sigs.find((s) => s.id === l.config.newSignatureId)?.label ??
                    l.config.newSignatureId;
                  return (
                    <option key={l.config.id} value={l.config.id}>
                      {l.config.label} — {oldLabel} → {newLabel}
                    </option>
                  );
                })}
              </select>

              <fieldset className="rounded-xl border border-border bg-bg-2/40 p-3">
                <legend className="px-1 text-sm font-medium text-text-0">
                  {t('sign.rotationSummary')}
                </legend>
                <p className="mb-3 text-xs text-text-2">{t('sign.rotationSummaryHint')}</p>
                <div className="grid gap-2 sm:grid-cols-2">
                  <SchemeOption
                    id="rotation-v1"
                    label={`V1 · ${t('sign.schemeV1')}`}
                    description={t('sign.rotationFixedOn')}
                    checked
                    disabled
                    onChange={() => undefined}
                  />
                  <SchemeOption
                    id="rotation-v2"
                    label={`V2 · ${t('sign.schemeV2')}`}
                    description={t('sign.rotationFixedOn')}
                    checked
                    disabled
                    onChange={() => undefined}
                  />
                  <SchemeOption
                    id="rotation-v3"
                    label={`V3 · ${t('sign.schemeV3')}`}
                    description={t('sign.rotationFixedOn')}
                    checked
                    disabled
                    onChange={() => undefined}
                  />
                  <SchemeOption
                    id="rotation-v31"
                    label={`V3.1 · ${t('sign.schemeV31')}`}
                    description={t('sign.rotationV31Desc', {
                      sdk: ROTATION_MIN_SDK_VERSION,
                    })}
                    checked
                    disabled
                    onChange={() => undefined}
                  />
                  <SchemeOption
                    id="rotation-v4"
                    label={`V4 · ${t('sign.schemeV4')}`}
                    description={t('sign.rotationV4Desc')}
                    checked={rotationV4}
                    disabled={busy || stripping}
                    onChange={setRotationV4}
                  />
                </div>
              </fieldset>

              {!lineageUsable && (
                <div
                  role="alert"
                  className="flex items-start gap-2 rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm font-medium text-slate-900 shadow-sm"
                >
                  <span className="mt-0.5 text-base leading-none text-slate-500">!</span>
                  <span className="leading-relaxed text-slate-900">
                    {selectedLineage
                      ? t('sign.rotationLineageUnavailable', {
                          reason: !selectedLineage.fileExists
                            ? t('sign.reasonFileMissing')
                            : !selectedLineage.oldSignatureExists
                            ? t('sign.reasonOldMissing')
                            : t('sign.reasonNewMissing'),
                        })
                      : t('sign.rotationNoLineage')}
                  </span>
                </div>
              )}

              <div className="flex flex-wrap gap-2">
                <Button
                  disabled={busy || !apk || !lineageUsable}
                  onClick={() => void startRotation()}
                >
                  {busy
                    ? t(checking ? 'sign.checking' : 'sign.starting')
                    : t('sign.startRotation')}
                </Button>
              </div>
            </TabsContent>
          </Tabs>
        </CardContent>
      </Card>

      <Dialog
        open={pendingResign !== null}
        onOpenChange={(open) => {
          if (!open && !busy) setPendingResign(null);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('sign.resignTitle')}</DialogTitle>
            <DialogDescription>
              {pendingResign
                ? pendingResign.kind === 'standard'
                  ? t('sign.resignDescription', { label: pendingResign.signatureLabel })
                  : t('sign.resignDescriptionRotation', {
                      label: pendingResign.lineageLabel,
                      old: pendingResign.oldSignatureLabel,
                      new: pendingResign.newSignatureLabel,
                    })
                : ''}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setPendingResign(null)}
              disabled={busy}
            >
              {t('common.cancel')}
            </Button>
            <Button variant="danger" onClick={() => void confirmResign()} disabled={busy}>
              {busy ? t('sign.starting') : t('sign.resignConfirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={pendingRisk !== null}
        onOpenChange={(open) => {
          if (!open) cancelRisk();
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('sign.riskTitle')}</DialogTitle>
            <DialogDescription>
              {riskKey ? t(`sign.${riskKey}`) : ''}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={cancelRisk} disabled={busy}>
              {t('common.cancel')}
            </Button>
            <Button variant="danger" onClick={() => void confirmRisk()} disabled={busy}>
              {busy ? t('sign.starting') : t('sign.riskConfirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={signatureInfo !== null}
        onOpenChange={(open) => {
          if (!open) closeInspect();
        }}
      >
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>{t('sign.inspectTitle')}</DialogTitle>
            <DialogDescription>
              {signatureInfo ? signatureInfo.apkPath : ''}
            </DialogDescription>
          </DialogHeader>
          {signatureInfo && (
            <div className="grid gap-2 text-sm">
              <div className="text-text-2">{t('sign.inspectFileSize', { size: signatureInfo.fileSize })}</div>
              <div className="flex flex-wrap gap-2">
                <SchemeBadge label="V1" verified={signatureInfo.verifiedV1} />
                <SchemeBadge label="V2" verified={signatureInfo.verifiedV2} />
                <SchemeBadge label="V3" verified={signatureInfo.verifiedV3} />
                <SchemeBadge label="V3.1" verified={signatureInfo.verifiedV31} />
                <SchemeBadge label="V4" verified={signatureInfo.verifiedV4} />
              </div>
              <div className="text-xs text-text-2">{t('sign.inspectSchemesHint')}</div>
              <div className="text-text-2">
                {signatureInfo.isSigned
                  ? t('sign.inspectSignerCount', { count: signatureInfo.signerCount })
                  : t('sign.inspectNotSigned')}
              </div>
              {!signatureInfo.verifies && !signatureInfo.isSigned && signatureInfo.errorMessage && (
                <div className="text-xs text-text-2">{signatureInfo.errorMessage}</div>
              )}
              <pre className="max-h-72 overflow-auto rounded-lg border border-border bg-bg-2/40 p-2 text-xs text-text-2">
                {signatureInfo.rawOutput || t('sign.inspectEmpty')}
              </pre>
            </div>
          )}
          <DialogFooter>
            <Button variant="outline" onClick={closeInspect} disabled={inspecting}>
              {t('common.close')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={pendingStrip !== null}
        onOpenChange={(open) => {
          if (!open) cancelStrip();
        }}
      >
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>{t('sign.stripConfirmTitle')}</DialogTitle>
            <DialogDescription>
              {t('sign.stripConfirmDescription')}
            </DialogDescription>
            <div className="break-all rounded-lg border border-border bg-bg-2/40 p-2 font-mono text-xs text-text-2">
              {pendingStrip}
            </div>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={cancelStrip} disabled={stripping}>
              {t('common.cancel')}
            </Button>
            <Button variant="danger" onClick={() => void confirmStrip()} disabled={stripping}>
              {stripping ? t('sign.stripping') : t('sign.stripConfirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={stripResult !== null}
        onOpenChange={(open) => {
          if (!open) closeStripResult();
        }}
      >
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>{t('sign.stripResultTitle')}</DialogTitle>
            <DialogDescription>
              {t('sign.stripResultDescription')}
            </DialogDescription>
          </DialogHeader>
          {stripResult && (
            <div className="grid gap-1 text-xs text-text-2">
              <div className="text-text-2">{t('sign.stripResultSourceLabel')}</div>
              <div className="break-all rounded-lg border border-border bg-bg-2/40 p-2 font-mono text-[11px]">
                {stripResult.sourcePath}
              </div>
              <div className="mt-2 text-text-2">{t('sign.stripResultOutputLabel')}</div>
              <div className="break-all rounded-lg border border-border bg-bg-2/40 p-2 font-mono text-[11px]">
                {stripResult.outputPath}
              </div>
              <div>
                {t('sign.stripResultSize', {
                  size: stripResult.outputSize,
                  removed: stripResult.removedV1Files.length,
                })}
              </div>
              <div>
                {t('sign.stripResultSchemes', {
                  v1: stripResult.hadV1,
                  v23: stripResult.hadV2V3,
                  v4: stripResult.hadV4,
                })}
              </div>
              {stripResult.removedV1Files.length > 0 && (
                <ul className="ml-4 list-disc">
                  {stripResult.removedV1Files.map((name) => (
                    <li key={name}>{name}</li>
                  ))}
                </ul>
              )}
            </div>
          )}
          <DialogFooter>
            <Button variant="outline" onClick={closeStripResult}>
              {t('common.close')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

function SchemeBadge({ label, verified }: { label: string; verified: boolean }) {
  return (
    <span
      className={
        'rounded-md border px-2 py-0.5 text-xs ' +
        (verified
          ? 'border-emerald-500/40 bg-emerald-500/10 text-emerald-400'
          : 'border-border bg-bg-2/40 text-text-2')
      }
    >
      {label} · {verified ? '✓' : '—'}
    </span>
  );
}

function SchemeOption({
  id,
  label,
  description,
  checked,
  disabled,
  onChange,
}: {
  id: string;
  label: string;
  description: string;
  checked: boolean;
  disabled: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label
      htmlFor={id}
      className="flex items-start gap-2 rounded-lg border border-border bg-bg-1 p-2.5"
    >
      <input
        id={id}
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
        className="mt-0.5 h-4 w-4 shrink-0 accent-brand disabled:opacity-60"
      />
      <span className="min-w-0">
        <span className="block text-sm font-medium text-text-0">{label}</span>
        <span className="block text-xs leading-relaxed text-text-2">{description}</span>
      </span>
    </label>
  );
}

function defaultStrippedName(apkPath: string): string {
  const parts = apkPath.split(/[\\/]/);
  const last = parts[parts.length - 1] || 'apk.apk';
  const dot = last.lastIndexOf('.');
  const stem = dot > 0 ? last.slice(0, dot) : last;
  return `${stem}.unsigned.apk`;
}
