import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import type { LucideIcon } from 'lucide-react';
import {
  Box,
  Layers,
  ShieldCheck,
  FileSearch,
  ArrowUpRight,
  Info,
  HardDrive,
  Gauge,
  Activity,
  Server,
  Inbox,
  Database,
  ExternalLink,
  ChevronDown,
  ChevronRight,
  AlertTriangle,
} from 'lucide-react';
import { Card, CardContent } from '@/components/ui/card';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';
import type { ApkInfo, SecurityReport, VolumeStats } from '@/ipc/analyze';
import type { PackerIndicator, PackerReport, SignatureInfo, SignerDetail } from '@/types/signing';
import type { ComponentHit, MatchedRule, RuleReport } from '@/ipc/rules';
import { VolumePanel } from './VolumePanel';
import { SecurityPanel } from './SecurityPanel';

type Props = {
  info: ApkInfo;
  ruleReport?: RuleReport | null;
};

function countMatches(report: RuleReport | null | undefined): number {
  if (!report) return 0;
  return report.total_matched;
}

function totalComponents(info: ApkInfo): number {
  // Native libraries belong in the same "components" tab now -- they were
  // previously only exposed via the rules report. Counting them keeps the
  // dashboard badge consistent with the layout.
  return (
    info.activities.length +
    info.services.length +
    info.receivers.length +
    info.providers.length +
    (info.native_libs?.length ?? 0)
  );
}

/// Look up the `MatchedRule` (if any) for a given component on the
/// canonical APK basis. Returns `undefined` when `ruleReport` is null
/// or the component was not enumerated by the backend -- the caller
/// treats that as "no rule hit" rather than "rule hit missing data".
function findHit(
  report: RuleReport | null | undefined,
  kind: 'native_libraries' | 'activities' | 'services' | 'receivers' | 'providers',
  name: string,
): ComponentHit | undefined {
  return report?.components?.[kind]?.find((h) => h.name === name);
}

function scoreColor(score: number | null | undefined): string {
  if (score == null) return 'var(--text-2, #94a3b8)';
  if (score >= 85) return 'var(--success, #22c55e)';
  if (score >= 60) return 'var(--warning, #f59e0b)';
  return 'var(--danger, #ef4444)';
}

function scoreLabelKey(score: number | null | undefined): string {
  if (score == null) return 'analyze.dashSecurityNoScan';
  if (score >= 85) return 'analyze.dashSecurityGood';
  if (score >= 60) return 'analyze.dashSecurityWarn';
  return 'analyze.dashSecurityBad';
}

function formatSize(bytes: number | null | undefined): string {
  if (!bytes) return '0 B';
  const k = 1024;
  const units = ['B', 'KB', 'MB', 'GB'];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), units.length - 1);
  const v = bytes / Math.pow(k, i);
  return `${v.toFixed(2)} ${units[i]}`;
}

function AppAvatar({ label }: { label: string }) {
  // JADB 当前 ApkInfo 不携带 icon 数据；用首字母 + 渐变占位，风格与 VSKiller
  // 的 80x80 应用图标位保持一致。后续若后端加 iconBase64 字段只需替换。
  const initial = (label || '?').trim().charAt(0).toUpperCase() || '?';
  return (
    <div
      className={cn(
        'grid h-16 w-16 shrink-0 place-items-center rounded-2xl',
        'bg-gradient-to-br from-brand/30 to-brand/0 text-brand-strong',
        'ring-1 ring-border shadow-card text-2xl font-bold',
      )}
      aria-hidden
    >
      {initial}
    </div>
  );
}

function DashCard({
  label,
  value,
  unit,
  desc,
  icon: Icon,
  accent,
  onClick,
}: {
  label: string;
  value: string | number;
  unit?: string | undefined;
  desc: string;
  icon: typeof Box;
  accent: string;
  onClick?: () => void;
}) {
  const Tag = onClick ? 'button' : 'div';
  return (
    <Tag
      type={onClick ? 'button' : undefined}
      onClick={onClick}
      className={cn(
        'group relative flex w-full items-center gap-4 overflow-hidden rounded-2xl border border-border bg-bg-1 p-5 text-left',
        'shadow-card transition-all duration-200',
        onClick &&
          'cursor-pointer hover:-translate-y-0.5 hover:border-brand hover:shadow-[0_18px_40px_rgba(15,23,42,0.12)]',
      )}
    >
      <div
        className={cn(
          'pointer-events-none absolute inset-0 bg-gradient-to-br opacity-60 transition-opacity',
          'group-hover:opacity-100',
          accent,
        )}
        aria-hidden
      />
      <div
        className={cn(
          'relative grid h-11 w-11 place-items-center rounded-xl bg-bg-2 ring-1 ring-border text-brand',
        )}
      >
        <Icon className="h-5 w-5" />
      </div>
      <div className="relative flex-1 min-w-0">
        <div className="text-[11px] font-semibold uppercase tracking-wider text-text-2">
          {label}
        </div>
        <div className="mt-0.5 flex items-baseline gap-1 text-text-0">
          <span className="text-2xl font-extrabold leading-none">{value}</span>
          {unit && <span className="text-sm font-medium text-text-2">{unit}</span>}
        </div>
        <div className="mt-1 truncate text-xs text-text-2">{desc}</div>
      </div>
      {onClick && (
        <ArrowUpRight
          className="relative h-4 w-4 shrink-0 text-text-2 transition-all group-hover:-translate-y-0.5 group-hover:translate-x-0.5 group-hover:text-text-0"
          aria-hidden
        />
      )}
    </Tag>
  );
}

function MetaRow({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="flex items-baseline gap-3 border-b border-border/60 py-2 last:border-0">
      <span className="w-24 shrink-0 text-xs text-text-2">{label}</span>
      <span className="min-w-0 break-all font-mono text-sm text-text-0">{value}</span>
    </div>
  );
}

/// Severity -> Tailwind accent classes. Unmatched components stay muted.
const SEVERITY_CLS: Record<string, string> = {
  critical: 'bg-red-500/15 text-red-500',
  danger: 'bg-red-500/15 text-red-500',
  error: 'bg-red-500/15 text-red-500',
  high: 'bg-red-500/15 text-red-500',
  warning: 'bg-amber-500/15 text-amber-500',
  warn: 'bg-amber-500/15 text-amber-500',
  medium: 'bg-amber-500/15 text-amber-500',
  info: 'bg-sky-500/15 text-sky-500',
  low: 'bg-sky-500/15 text-sky-500',
  note: 'bg-sky-500/15 text-sky-500',
};

function severityClass(sev: string | null | undefined): string {
  if (!sev) return 'bg-bg-2 text-text-2';
  return SEVERITY_CLS[sev.toLowerCase()] ?? 'bg-brand/15 text-brand-strong';
}

function HitDetail({ hit }: { hit: MatchedRule }) {
  // Render the rule metadata that the LibChecker / bundled packs surface.
  // For unknown severity strings we fall back to a neutral badge instead
  // of hiding the row -- the description still carries the meaning.
  const label = hit.metadata?.label;
  const team = hit.metadata?.dev_team;
  const link = hit.metadata?.source_link;
  const zh = hit.metadata?.zh_description;
  return (
    <div className="ml-3 mr-1 mb-1 rounded-md border border-dashed border-border bg-bg-2/60 px-3 py-2 text-xs leading-relaxed">
      <div className="flex flex-wrap items-center gap-2">
        <Badge className={cn('border-0', severityClass(hit.severity))}>
          {hit.severity}
        </Badge>
        {label && <span className="font-semibold text-text-0">{label}</span>}
        {team && <span className="text-text-2">· {team}</span>}
        {link && (
          <a
            href={link}
            target="_blank"
            rel="noreferrer noopener"
            className="inline-flex items-center gap-1 text-brand hover:underline"
          >
            <ExternalLink className="h-3 w-3" />
            source
          </a>
        )}
      </div>
      {hit.description && hit.description !== label && (
        <div className="mt-1 text-text-1">{hit.description}</div>
      )}
      {zh && zh !== hit.description && (
        <div className="mt-1 text-text-2">{zh}</div>
      )}
      <div className="mt-1 font-mono text-[10px] text-text-2">
        {hit.rule_set_id} / {hit.rule_id}
      </div>
    </div>
  );
}

function ComponentList({
  title,
  hits,
  accent,
  Icon,
  expanded = false,
}: {
  title: string;
  hits: ComponentHit[];
  accent: string;
  Icon: LucideIcon;
  /** When true, the row list grows to fill the viewport. Used when the list
   *  owns its own tab and there is no second column to stay beside it. */
  expanded?: boolean;
}) {
  const matched = hits.filter((h) => h.matched_rule).length;
  // Track which row is expanded so we don't ship the whole closed payload to
  // the DOM -- rule metadata can be hefty and most rows are usually leaves.
  const [openName, setOpenName] = useState<string | null>(null);

  return (
    <div className="rounded-xl border border-border bg-bg-1/60 p-4">
      <div className="mb-2 flex items-center justify-between">
        <span className="flex items-center gap-1.5 text-sm font-semibold text-text-0">
          <Icon className="h-3.5 w-3.5 text-text-1" />
          {title}
        </span>
        <div className="flex items-center gap-1.5">
          {matched > 0 && (
            <Badge variant="danger" className="text-[10px]">
              {matched} {matched === 1 ? 'hit' : 'hits'}
            </Badge>
          )}
          <span className={cn('rounded-md px-2 py-0.5 text-xs font-bold', accent)}>
            {hits.length}
          </span>
        </div>
      </div>
      {hits.length === 0 ? (
        <div className="text-xs text-text-2">—</div>
      ) : (
        <div
          className={cn(
            'overflow-auto font-mono text-xs leading-relaxed',
            expanded
              ? 'max-h-[calc(100vh-22rem)]'
              : 'max-h-72',
          )}
        >
          {hits.map((hit) => {
            const matched_rule = hit.matched_rule;
            const isOpen = matched_rule && openName === hit.name;
            return (
              <div key={hit.name}>
                <button
                  type="button"
                  onClick={() =>
                    matched_rule &&
                    setOpenName(isOpen ? null : hit.name)
                  }
                  disabled={!matched_rule}
                  className={cn(
                    'flex w-full items-start gap-1.5 break-all rounded px-2 py-1 text-left',
                    matched_rule
                      ? 'cursor-pointer text-text-0 hover:bg-bg-2'
                      : 'cursor-default text-text-1 hover:bg-bg-2/40',
                  )}
                >
                  {matched_rule ? (
                    isOpen ? (
                      <ChevronDown className="mt-0.5 h-3 w-3 shrink-0 text-text-2" />
                    ) : (
                      <ChevronRight className="mt-0.5 h-3 w-3 shrink-0 text-text-2" />
                    )
                  ) : (
                    <span className="mt-0.5 inline-block h-3 w-3 shrink-0" />
                  )}
                  <span className="flex-1">{hit.name}</span>
                  {matched_rule && (
                    <Badge
                      className={cn(
                        'shrink-0 border-0 text-[10px]',
                        severityClass(matched_rule.severity),
                      )}
                    >
                      {matched_rule.severity}
                    </Badge>
                  )}
                </button>
                {isOpen && matched_rule && (
                  <HitDetail hit={matched_rule} />
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}


/// Per-category tab trigger. Reuses the project's `TabsTrigger` styling and
/// surfaces (a) the icon for the kind, (b) a muted total count, (c) a
/// red-tinted hit badge when rule matching produced results. Both badges are
/// hidden when zero so the bar doesn't drown in pills for an empty APK.
function CategoryTabTrigger({
  value,
  activeIcon: Icon,
  label,
  count,
  hits,
}: {
  value: string;
  activeIcon: LucideIcon;
  label: string;
  count: number;
  hits: number;
}) {
  return (
    <TabsTrigger
      value={value}
      className="rounded-b-none data-[state=active]:border-b-2 data-[state=active]:border-brand"
    >
      <Icon className="mr-1 inline h-3.5 w-3.5 text-text-1" />
      {label}
      <span className="ml-1.5 rounded bg-bg-2 px-1.5 text-[10px] font-bold text-text-1">
        {count}
      </span>
      {hits > 0 && (
        <span className="ml-1 rounded bg-danger/15 px-1.5 text-[10px] font-bold text-danger">
          {hits}
        </span>
      )}
    </TabsTrigger>
  );
}

/// Same content as <ComponentList /> but presented full-width inside its own
/// tab. We rely on `ComponentList.expanded = true` so the row list can grow
/// up to nearly viewport height instead of the cramped 72px-feel of a side
/// column inside the multi-grid layout.
function FullWidthCategoryPanel({
  Icon,
  title,
  accent,
  hits,
}: {
  Icon: LucideIcon;
  title: string;
  accent: string;
  hits: ComponentHit[];
}) {
  return (
    <ComponentList
      title={title}
      Icon={Icon}
      accent={accent}
      hits={hits}
      expanded
    />
  );
}

/// Returns true when the signer record is so thin we should hide it from
/// the UI entirely. apksigner occasionally emits `Signer #N` rows whose
/// only contribution is a single scheme verdict; those would otherwise
/// render as empty cards.
function hasSignerContent(s: SignerDetail): boolean {
  return Boolean(
    s.dn || s.issuerDn || s.sha256 || s.sha1 || s.md5 || s.serial ||
    s.validFrom || s.validTo || s.keyAlgorithm || s.publicKeySha256 ||
    s.publicKeySha1 || s.publicKeyMd5 || s.signatureAlgorithm ||
    s.certVersion != null || s.warnings.length > 0,
  );
}

/// Coloured chip for the key-strength bucket. The `bits` and `algorithm`
/// props are forwarded by the parent so future iterations can render a
/// tooltip with the actual algorithm name; the chip itself only needs
/// the bucket to pick a colour.
function KeyStrengthBadge({
  strength,
  bits: _bits,
  algorithm: _algorithm,
}: {
  strength: SignerDetail['keyStrength'];
  bits: number | null;
  algorithm: string | null;
}) {
  // Map our backend buckets to palette tokens so the chip colour matches
  // the existing severity scheme on the rest of the analysis page. The
  // raw algorithm / bit length are forwarded here only as a hook for a
  // future tooltip; the chip itself only depends on the bucket.
  const PALETTE: Record<SignerDetail['keyStrength'], string> = {
    weak: 'bg-red-500/15 text-red-500',
    acceptable: 'bg-amber-500/15 text-amber-500',
    strong: 'bg-emerald-500/15 text-emerald-500',
    unknown: 'bg-bg-2 text-text-2',
  };
  return (
    <Badge className={cn('border-0', PALETTE[strength])}>
      {strength.toUpperCase()}
    </Badge>
  );
}

/// Full per-signer certificate card. Layout:
///
///   [Signer #N]  [v1] [v2]  <scheme badges>
///   [Debug key warning]  <-- when isDebugSigned
///   Subject DN          Issuer DN
///   Cert SHA-256 / SHA-1 / MD5
///   Public Key SHA-256 / SHA-1 / MD5
///   Algorithm   Key bits   [STRENGTH]
///   Signature algorithm   Serial   X.509 version
///   Validity start / end
///   Warnings (v1-only "META-INF not protected" rows)
function SignerRow({ signer }: { signer: SignerDetail }) {
  const { t } = useTranslation();

  if (!hasSignerContent(signer)) {
    // Pure scheme-verdict row -- nothing visible to render.
    return null;
  }

  return (
    <div className="space-y-3 rounded-xl border border-border bg-bg-1/60 p-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2 text-sm font-semibold text-text-0">
          {t('analyze.signer')} #{signer.index}
          {signer.isDebugSigned && (
            <Badge className="border-0 bg-amber-500/15 text-amber-500">
              {t('analyze.debugKey')}
            </Badge>
          )}
        </div>
        <div className="flex flex-wrap gap-1">
          {signer.schemes.map((s) => (
            <Badge key={s} variant="secondary" className="text-[10px] font-mono">
              {s}
            </Badge>
          ))}
        </div>
      </div>

      <dl className="grid grid-cols-[max-content_1fr] gap-x-3 gap-y-1.5 text-xs">
        {signer.dn && <Field label={t('analyze.signerDn')} value={signer.dn} />}
        {signer.issuerDn && (
          <Field label={t('analyze.signerIssuerDn')} value={signer.issuerDn} />
        )}
        {signer.sha256 && <Field label="Cert SHA-256" value={signer.sha256} mono />}
        {signer.sha1 && <Field label="Cert SHA-1" value={signer.sha1} mono />}
        {signer.md5 && <Field label="Cert MD5" value={signer.md5} mono />}
        {signer.publicKeySha256 && (
          <Field label={t('analyze.signerPubkeySha256')} value={signer.publicKeySha256} mono />
        )}
        {signer.publicKeySha1 && (
          <Field label={t('analyze.signerPubkeySha1')} value={signer.publicKeySha1} mono />
        )}
        {signer.publicKeyMd5 && (
          <Field label={t('analyze.signerPubkeyMd5')} value={signer.publicKeyMd5} mono />
        )}
        {signer.keyAlgorithm && (
          <Field label={t('analyze.signerAlgorithm')} value={signer.keyAlgorithm} />
        )}
        {signer.keyBits != null && (
          <Field label={t('analyze.signerKeyBits')} value={String(signer.keyBits)} />
        )}
        <dt className="text-text-2">{t('analyze.signerKeyStrength')}</dt>
        <dd>
          <KeyStrengthBadge
            strength={signer.keyStrength}
            bits={signer.keyBits}
            algorithm={signer.keyAlgorithm}
          />
        </dd>
        {signer.signatureAlgorithm && (
          <Field label={t('analyze.signerSigAlg')} value={signer.signatureAlgorithm} />
        )}
        {signer.serial && (
          <Field label={t('analyze.signerSerial')} value={signer.serial} mono />
        )}
        {signer.certVersion != null && (
          <Field
            label={t('analyze.signerCertVersion')}
            value={`v${signer.certVersion}`}
          />
        )}
        {signer.validFrom && (
          <Field label={t('analyze.signerValidFrom')} value={signer.validFrom} />
        )}
        {signer.validTo && (
          <Field label={t('analyze.signerValidTo')} value={signer.validTo} />
        )}
      </dl>

      {signer.warnings.length > 0 && (
        <div className="rounded-md border border-amber-500/30 bg-amber-500/5 p-2 text-xs leading-relaxed text-amber-500">
          <div className="mb-1 font-semibold">{t('analyze.signerWarnings')}</div>
          <ul className="space-y-0.5">
            {signer.warnings.map((w: string, i: number) => (
              <li key={i} className="break-all">• {w}</li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

function Field({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <>
      <dt className="text-text-2">{label}</dt>
      <dd className={cn('break-all', mono ? 'font-mono text-text-1' : 'text-text-0')}>
        {value}
      </dd>
    </>
  );
}

/// Top-level status header. Uses the same severity palette as ComponentList
/// for negative / neutral / positive state.
function SignatureStatusBar({ signature }: { signature: SignatureInfo | null | undefined }) {
  const { t } = useTranslation();
  if (!signature) {
    return (
      <div className="rounded-xl border border-dashed border-border p-4 text-sm text-text-2">
        {t('analyze.sigUnavailable')}
      </div>
    );
  }
  if (signature.errorMessage) {
    return (
      <div className="rounded-xl border border-amber-500/40 bg-amber-500/10 p-4 text-sm text-amber-500">
        {signature.errorMessage}
      </div>
    );
  }
  if (!signature.isSigned) {
    return (
      <div className="rounded-xl border border-dashed border-border p-4 text-sm text-text-2">
        {t('analyze.sigUnsigned')}
      </div>
    );
  }
  const verified: Array<{ key: string; on: boolean }> = [
    { key: 'v1', on: signature.verifiedV1 },
    { key: 'v2', on: signature.verifiedV2 },
    { key: 'v3', on: signature.verifiedV3 },
    { key: 'v3.1', on: signature.verifiedV31 },
    { key: 'v4', on: signature.verifiedV4 },
  ];
  return (
    <div className="rounded-xl border border-border bg-bg-1/60 p-4">
      <div className="mb-2 flex flex-wrap items-center gap-2 text-sm font-semibold text-text-0">
        <ShieldCheck className="h-4 w-4 text-success" />
        {t('analyze.sigSignedOk')}
        <span className="ml-auto font-mono text-xs text-text-2">
          {signature.signerCount} {t('analyze.signers')}
        </span>
      </div>
      <div className="flex flex-wrap gap-1.5">
        {verified.map((s) => (
          <Badge
            key={s.key}
            variant={s.on ? 'success' : 'secondary'}
            className="font-mono text-[10px]"
          >
            {s.key}
          </Badge>
        ))}
      </div>
    </div>
  );
}

/// Packer / shell detection panel. The detector is heuristic so we always
/// show what fired: even when `isPacked` is false we still render an
/// explicit "not detected" panel rather than hiding the section, so
/// users know the pipeline ran.
function PackerPanel({ report }: { report: PackerReport | null | undefined }) {
  const { t } = useTranslation();
  if (!report) {
    return (
      <div className="rounded-xl border border-dashed border-border p-4 text-sm text-text-2">
        {t('analyze.sigUnavailable')}
      </div>
    );
  }
  if (!report.isPacked) {
    return (
      <div className="flex items-center gap-2 rounded-xl border border-border bg-bg-1/60 p-4 text-sm text-text-1">
        <ShieldCheck className="h-4 w-4 text-success" />
        {t('analyze.packerClean')}
      </div>
    );
  }
  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2 rounded-xl border border-red-500/40 bg-red-500/10 p-4">
        <AlertTriangle className="h-4 w-4 text-red-500" />
        <span className="text-sm font-semibold text-red-500">
          {t('analyze.packerDetected')}
          {report.packerName ? `: ${report.packerName}` : ''}
        </span>
      </div>
      <div className="rounded-xl border border-border bg-bg-1/60 p-4">
        <div className="mb-2 text-sm font-semibold text-text-0">
          {t('analyze.packerIndicators')}
        </div>
        <ul className="space-y-1 font-mono text-xs leading-relaxed text-text-1">
          {report.indicators.map((it: PackerIndicator, i: number) => (
            <li key={`${it.kind}:${it.value}:${i}`} className="break-all rounded px-2 py-1 hover:bg-bg-2">
              <Badge variant="outline" className="mr-2 text-[10px]">
                {it.packer}
              </Badge>
              <span className="text-text-2">{it.kind}: </span>
              {it.value}
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}

/// The signing tab content. Three sub-blocks stacked vertically:
/// top-level status, per-signer detail, packer detection.
function SigningPanel({ info }: { info: ApkInfo }) {
  // Sub-components (`SignatureStatusBar`, `SignerRow`, `PackerPanel`)
  // each bind their own `useTranslation()`; this panel is a layout-only
  // shell and currently needs no strings of its own.
  const sig = info.signature ?? null;
  return (
    <div className="space-y-3">
      <SignatureStatusBar signature={sig} />
      {sig && sig.signers && sig.signers.length > 0 && (
        <div className="grid gap-3 md:grid-cols-2">
          {sig.signers.map((signer) => (
            <SignerRow key={signer.index} signer={signer} />
          ))}
        </div>
      )}
      <PackerPanel report={info.packer ?? null} />
    </div>
  );
}


function SectionCard({
  title,
  icon: Icon,
  children,
}: {
  title: string;
  icon: typeof Info;
  children: React.ReactNode;
}) {
  return (
    <Card>
      <div className="mb-3 flex items-center gap-2 text-text-0">
        <Icon className="h-4 w-4 text-brand" />
        <h3 className="text-sm font-semibold">{title}</h3>
      </div>
      <CardContent className="p-0">{children}</CardContent>
    </Card>
  );
}

function riskLevelCounts(report: SecurityReport | null | undefined) {
  const r = { critical: 0, warning: 0, info: 0 };
  if (!report) return r;
  for (const k of report.risks) {
    if (k.level === 'critical') r.critical++;
    else if (k.level === 'warning') r.warning++;
    else r.info++;
  }
  return r;
}

function volumeWasteText(stats: VolumeStats | null | undefined): string | null {
  if (!stats || stats.waste_size <= 0) return null;
  return formatSize(stats.waste_size);
}

export function ApkInfoCard({ info, ruleReport }: Props) {
  const { t } = useTranslation();

  const label = info.application_label?.trim() || info.package_name;
  const version =
    info.version_name || info.version_code
      ? `v${info.version_name ?? '?'} (${info.version_code ?? '?'})`
      : '—';
  const sdk =
    info.min_sdk || info.target_sdk
      ? `Min ${info.min_sdk ?? '?'} / Target ${info.target_sdk ?? '?'}${
          info.max_sdk ? ` / Max ${info.max_sdk}` : ''
        }`
      : '—';
  const componentCount = totalComponents(info);
  const matchCount = countMatches(ruleReport);
  const ruleSetCount = 0; // rule_set 数量不再透出到 UI；后续若需要可从 components.matched_rule.rule_set_id 去重
  const security: SecurityReport | null = info.security_report ?? null;
  const securityScore = security?.score ?? null;
  const risks = riskLevelCounts(security);
  const techStack = info.tech_stack ?? [];
  const waste = volumeWasteText(info.volume_stats);

  return (
    <div className="space-y-6 anim-rise">
      {/* App header — mirrors VSKiller's header block */}
      <div className="relative overflow-hidden rounded-2xl border border-border bg-bg-1 p-6 shadow-card">
        <div
          className="pointer-events-none absolute inset-0 bg-gradient-to-br from-brand/10 via-transparent to-brand-hot/10"
          aria-hidden
        />
        <div className="relative flex items-start gap-5">
          <AppAvatar label={label} />
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
              <h1 className="truncate text-2xl font-bold text-text-0">
                {label}
              </h1>
              <Badge variant="default" className="font-mono">
                {info.package_name}
              </Badge>
              <Badge variant="outline">{version}</Badge>
            </div>
            <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-text-2">
              <span>
                <span className="text-text-1">SDK</span>{' '}
                <span className="font-mono text-text-0">{sdk}</span>
              </span>
            </div>
            {techStack.length > 0 && (
              <div className="mt-3 flex flex-wrap gap-1.5">
                {techStack.map((s) => (
                  <Badge key={s} variant="secondary" className="font-mono">
                    {s}
                  </Badge>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Dashboard cards */}
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-4">
        <DashCard
          label={t('analyze.dashboardBasic')}
          value={7}
          unit="fields"
          desc={t('analyze.dashboardTotalDesc')}
          icon={Info}
          accent="from-sky-500/15 to-sky-500/0"
        />
        <DashCard
          label={t('analyze.dashboardPerms')}
          value={info.permissions.length}
          unit="perms"
          desc={t('analyze.permCountDesc')}
          icon={ShieldCheck}
          accent="from-emerald-500/15 to-emerald-500/0"
        />
        <DashCard
          label={t('analyze.dashboardComponents')}
          value={componentCount}
          unit="items"
          desc={t('analyze.compCountDesc')}
          icon={Layers}
          accent="from-indigo-500/15 to-indigo-500/0"
        />
        <DashCard
          label={t('analyze.dashboardRules')}
          value={matchCount}
          unit={ruleSetCount > 0 ? `/${ruleSetCount} packs` : undefined}
          desc={
            matchCount > 0
              ? t('analyze.ruleHitDesc')
              : t('analyze.ruleHitEmpty')
          }
          icon={FileSearch}
          accent="from-amber-500/15 to-amber-500/0"
        />
      </div>

      {/* Secondary dashboard: volume + security */}
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <DashCard
          label={t('analyze.dashVolume')}
          value={formatSize(info.volume_total_size ?? 0)}
          desc={
            waste
              ? t('analyze.dashVolumeWasteDesc', { size: waste })
              : t('analyze.dashVolumeCleanDesc')
          }
          icon={HardDrive}
          accent="from-cyan-500/15 to-cyan-500/0"
        />
        <DashCard
          label={t('analyze.dashSecurity')}
          value={securityScore != null ? securityScore : '—'}
          unit={securityScore != null ? '/ 100' : undefined}
          desc={t(scoreLabelKey(securityScore), {
            count: risks.critical + risks.warning,
          })}
          icon={Gauge}
          accent="from-rose-500/15 to-rose-500/0"
        />
      </div>

      {/* Tabs — body of the report */}
      <Card className="p-0">
        <Tabs defaultValue="overview" className="w-full">
          <div className="border-b border-border px-4 pt-3">
            <TabsList className="scrollbar-thin flex w-full justify-start gap-1 overflow-x-auto bg-transparent p-0">
              <TabsTrigger
                value="overview"
                className="rounded-b-none data-[state=active]:border-b-2 data-[state=active]:border-brand"
              >
                {t('analyze.tabOverview')}
              </TabsTrigger>
              <TabsTrigger
                value="permissions"
                className="rounded-b-none data-[state=active]:border-b-2 data-[state=active]:border-brand"
              >
                {t('analyze.tabPermissions')}
                <span className="ml-1.5 rounded bg-bg-2 px-1.5 text-[10px] font-bold text-text-1">
                  {info.permissions.length}
                </span>
              </TabsTrigger>
              <CategoryTabTrigger
                value="activities"
                activeIcon={Activity}
                label="Activities"
                count={info.activities.length}
                hits={
                  ruleReport?.components?.activities?.filter((h) => !!h.matched_rule).length ?? 0
                }
              />
              <CategoryTabTrigger
                value="services"
                activeIcon={Server}
                label="Services"
                count={info.services.length}
                hits={
                  ruleReport?.components?.services?.filter((h) => !!h.matched_rule).length ?? 0
                }
              />
              <CategoryTabTrigger
                value="receivers"
                activeIcon={Inbox}
                label="Receivers"
                count={info.receivers.length}
                hits={
                  ruleReport?.components?.receivers?.filter((h) => !!h.matched_rule).length ?? 0
                }
              />
              <CategoryTabTrigger
                value="providers"
                activeIcon={Database}
                label="Providers"
                count={info.providers.length}
                hits={
                  ruleReport?.components?.providers?.filter((h) => !!h.matched_rule).length ?? 0
                }
              />
              <CategoryTabTrigger
                value="native_libs"
                activeIcon={Box}
                label={t('analyze.tabNativeLibs')}
                count={info.native_libs?.length ?? 0}
                hits={
                  ruleReport?.components?.native_libraries?.filter((h) => !!h.matched_rule).length ?? 0
                }
              />              <TabsTrigger
                value="volume"
                className="rounded-b-none data-[state=active]:border-b-2 data-[state=active]:border-brand"
              >
                {t('analyze.tabVolume')}
              </TabsTrigger>
              <TabsTrigger
                value="security"
                className="rounded-b-none data-[state=active]:border-b-2 data-[state=active]:border-brand"
              >
                {t('analyze.tabSecurity')}
                {risks.critical + risks.warning > 0 && (
                  <span
                    className="ml-1.5 rounded px-1.5 text-[10px] font-bold"
                    style={{ backgroundColor: scoreColor(securityScore) + '22', color: scoreColor(securityScore) }}
                  >
                    {risks.critical + risks.warning}
                  </span>
                )}
              </TabsTrigger>
              <TabsTrigger
                value="signing"
                className="rounded-b-none data-[state=active]:border-b-2 data-[state=active]:border-brand"
              >
                {t('analyze.tabSigning')}
                {info.packer?.isPacked && (
                  <span className="ml-1.5 rounded bg-red-500/15 px-1.5 text-[10px] font-bold text-red-500">
                    {t('analyze.hardened')}
                  </span>
                )}
              </TabsTrigger>
            </TabsList>
          </div>

          <div className="p-6">
            <TabsContent value="overview" className="mt-0 space-y-4">
              <SectionCard title={t('analyze.basicInfo')} icon={Info}>
                <div>
                  <MetaRow label={t('analyze.package')} value={info.package_name} />
                  <MetaRow label={t('analyze.appName')} value={info.application_label ?? '—'} />
                  <MetaRow
                    label={t('analyze.versionName')}
                    value={info.version_name ?? '—'}
                  />
                  <MetaRow
                    label={t('analyze.versionCode')}
                    value={info.version_code ?? '—'}
                  />
                  <MetaRow label={t('analyze.minSdk')} value={info.min_sdk ?? '—'} />
                  <MetaRow
                    label={t('analyze.targetSdk')}
                    value={info.target_sdk ?? '—'}
                  />
                  <MetaRow label={t('analyze.maxSdk')} value={info.max_sdk ?? '—'} />
                </div>
              </SectionCard>
            </TabsContent>

            <TabsContent value="permissions" className="mt-0">
              {info.permissions.length === 0 ? (
                <div className="rounded-xl border border-dashed border-border p-8 text-center text-sm text-text-2">
                  {t('analyze.tabEmpty')}
                </div>
              ) : (
                <div className="flex flex-wrap gap-1.5">
                  {info.permissions.map((p) => (
                    <Badge key={p} variant="secondary" className="font-mono">
                      {p}
                    </Badge>
                  ))}
                </div>
              )}
            </TabsContent>

            <TabsContent value="activities" className="mt-0">
              <FullWidthCategoryPanel
                Icon={Activity}
                title="Activities"
                accent="bg-brand/15 text-brand-strong"
                hits={info.activities.map((name) => ({
                  name,
                  matched_rule: findHit(ruleReport, 'activities', name)?.matched_rule ?? null,
                }))}
              />
            </TabsContent>
            <TabsContent value="services" className="mt-0">
              <FullWidthCategoryPanel
                Icon={Server}
                title="Services"
                accent="bg-emerald-500/15 text-emerald-500"
                hits={info.services.map((name) => ({
                  name,
                  matched_rule: findHit(ruleReport, 'services', name)?.matched_rule ?? null,
                }))}
              />
            </TabsContent>
            <TabsContent value="receivers" className="mt-0">
              <FullWidthCategoryPanel
                Icon={Inbox}
                title="Receivers"
                accent="bg-amber-500/15 text-amber-500"
                hits={info.receivers.map((name) => ({
                  name,
                  matched_rule: findHit(ruleReport, 'receivers', name)?.matched_rule ?? null,
                }))}
              />
            </TabsContent>
            <TabsContent value="providers" className="mt-0">
              <FullWidthCategoryPanel
                Icon={Database}
                title="Providers"
                accent="bg-indigo-500/15 text-indigo-500"
                hits={info.providers.map((name) => ({
                  name,
                  matched_rule: findHit(ruleReport, 'providers', name)?.matched_rule ?? null,
                }))}
              />
            </TabsContent>
            <TabsContent value="native_libs" className="mt-0">
              <FullWidthCategoryPanel
                Icon={Box}
                title={t('analyze.tabNativeLibs')}
                accent="bg-sky-500/15 text-sky-500"
                hits={(info.native_libs ?? []).map((name) => ({
                  name,
                  matched_rule:
                    findHit(ruleReport, 'native_libraries', name)?.matched_rule ?? null,
                }))}
              />
            </TabsContent>

            <TabsContent value="volume" className="mt-0">
              <VolumePanel info={info} />
            </TabsContent>

            <TabsContent value="security" className="mt-0">
              <SecurityPanel info={info} />
            </TabsContent>


            <TabsContent value="signing" className="mt-0">
              <SigningPanel info={info} />
            </TabsContent>
          </div>
        </Tabs>
      </Card>
    </div>
  );
}
