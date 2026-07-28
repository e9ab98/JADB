import { useTranslation } from 'react-i18next';
import '@/i18n';
import { Card, CardContent } from '@/components/ui/card';
import type { ApkInfo, SecurityRisk, SecurityReport } from '@/ipc/analyze';

const LEVEL_ORDER: Record<string, number> = { critical: 0, warning: 1, info: 2 };

function levelColorVar(level: string): string {
  if (level === 'critical') return 'var(--danger, #ef4444)';
  if (level === 'warning') return 'var(--warning, #f59e0b)';
  return 'var(--info, #3b82f6)';
}

function levelBgVar(level: string): string {
  if (level === 'critical') return 'var(--danger, #ef4444)';
  if (level === 'warning') return 'var(--warning, #f59e0b)';
  return 'var(--info, #3b82f6)';
}

function ScoreGauge({ score }: { score: number }) {
  const { t } = useTranslation();
  const color = score >= 85 ? 'var(--success, #22c55e)' : score >= 60 ? 'var(--warning, #f59e0b)' : 'var(--danger, #ef4444)';
  return (
    <div className="relative h-[120px] w-[120px] shrink-0">
      <svg viewBox="0 0 36 36" className="h-full w-full" style={{ transform: 'rotate(-90deg)' }}>
        <path
          d="M18 2.0845 a 15.9155 15.9155 0 0 1 0 31.831 a 15.9155 15.9155 0 0 1 0 -31.831"
          fill="none"
          stroke="rgba(15, 23, 42, 0.08)"
          strokeWidth={3}
        />
        <path
          d="M18 2.0845 a 15.9155 15.9155 0 0 1 0 31.831 a 15.9155 15.9155 0 0 1 0 -31.831"
          fill="none"
          stroke={color}
          strokeWidth={3}
          strokeDasharray={`${score}, 100`}
        />
      </svg>
      <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center">
        <span className="text-2xl font-extrabold leading-none" style={{ color }}>
          {score}
        </span>
        <span className="text-[10px] text-text-2">{t("analyze.securityScore")}</span>
      </div>
    </div>
  );
}

function RiskCard({ risk }: { risk: SecurityRisk }) {
  const { t } = useTranslation();
  return (
    <div
      className="mb-3 rounded-xl bg-bg-1/40 p-4"
      style={{ borderLeft: `4px solid ${levelBgVar(risk.level)}` }}
    >
      <div className="mb-2 flex items-start justify-between gap-3">
        <h4 className="flex items-center gap-2 text-sm font-semibold text-text-0">
          <span
            className="rounded px-1.5 py-0.5 text-[10px] font-bold uppercase tracking-wider"
            style={{
              color: levelColorVar(risk.level),
              backgroundColor: levelColorVar(risk.level) + '22',
            }}
          >
            {risk.level}
          </span>
          {risk.title}
        </h4>
      </div>
      <p className="mb-2 text-xs text-text-1">{risk.description}</p>
      <div className="rounded bg-bg-2/60 p-2 text-[11px] text-text-1">
        <b className="text-text-0">{t("analyze.securitySuggestion")}: </b>
        {risk.suggestion}
      </div>
    </div>
  );
}

type Props = { info: ApkInfo };

export function SecurityPanel({ info }: Props) {
  const { t } = useTranslation();
  const report: SecurityReport | null | undefined = info.security_report;

  if (!report) {
    return (
      <div className="rounded-xl border border-dashed border-border p-8 text-center text-sm text-text-2">
        {t('analyze.noSecurityData')}
      </div>
    );
  }

  const sorted = [...report.risks].sort((a, b) => {
    return (LEVEL_ORDER[a.level] ?? 9) - (LEVEL_ORDER[b.level] ?? 9);
  });

  return (
    <div className="space-y-4">
      <Card>
        <CardContent className="flex flex-col items-center gap-6 p-6 md:flex-row md:gap-10">
          <ScoreGauge score={report.score} />
          <div className="flex-1">
            <h2 className="mb-2 text-lg font-semibold text-text-0">
              {t('analyze.securityOverview')}
            </h2>
            <p className="text-sm text-text-2">
              {report.risks.length === 0
                ? t('analyze.securityNoRisks')
                : t('analyze.securityFindings', { count: report.risks.length })}
            </p>
          </div>
        </CardContent>
      </Card>

      <div className="space-y-2">
        {sorted.length === 0 ? (
          <div className="rounded-xl border border-dashed border-border p-8 text-center text-sm text-text-2">
            {t('analyze.securityNoIssues')}
          </div>
        ) : (
          sorted.map((r) => <RiskCard key={r.id} risk={r} />)
        )}
      </div>
    </div>
  );
}
