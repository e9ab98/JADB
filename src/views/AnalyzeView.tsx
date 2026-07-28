import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';
import { open } from '@tauri-apps/plugin-dialog';
import '@/i18n';
import { toast } from 'sonner';
import {
  AlertTriangle,
  Download,
  FileSearch,
  FolderOpen,
  Loader2,
  RefreshCw,
  ScrollText,
  Settings as SettingsIcon,
  Sparkles,
} from 'lucide-react';
import { save as saveDialog } from '@tauri-apps/plugin-dialog';
import { exportApkReport } from '@/ipc/report';
import { buildReportHtml } from '@/features/apkAnalyze/reportTemplate';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { ApkInfoCard } from '@/features/apkAnalyze/ApkInfoCard';
import { analyzeApk, type ApkInfo } from '@/ipc/analyze';
import { fileSize } from '@/ipc/files';
import { listRules, analyzeWithRules, type RuleReport, type RuleSet } from '@/ipc/rules';
import { openPath } from '@/ipc/decompile';
import { getLogPath } from '@/ipc/files';
import { formatBytes, cn } from '@/lib/utils';
import { useSettingsStore } from '@/store/settings';

function isToolMissingAapt2(message: string): boolean {
  return /tool missing.*aapt2/i.test(message);
}

/**
 * Analyze view. Mirrors the VSKiller flow at a structural level — a top
 * app bar (file picker / repick) followed by the analyzer dashboard. The
 * heavy lifting still lives in `ApkInfoCard` which renders the header,
 * dashboard cards, and tabbed report.
 */
export function AnalyzeView() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const aaptPath = useSettingsStore((s) => s.settings?.aaptPath ?? null);
  const rulesPath = useSettingsStore((s) => s.settings?.rulesPath ?? null);

  const [path, setPath] = useState<string | null>(null);
  const [size, setSize] = useState<number | null>(null);
  const [info, setInfo] = useState<ApkInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [ruleSets, setRuleSets] = useState<RuleSet[]>([]);
  const [ruleReport, setRuleReport] = useState<RuleReport | null>(null);
  const [rulesBusy, setRulesBusy] = useState(false);

  async function pickAndAnalyze() {
    try {
      const picked = await open({
        multiple: false,
        filters: [{ name: 'APK', extensions: ['apk'] }],
      });
      if (typeof picked !== 'string') return;

      setPath(picked);
      setInfo(null);
      setError(null);
      setRuleReport(null);
      setSize(null);

      let bytes: number | null = null;
      try {
        bytes = await fileSize(picked);
        setSize(bytes);
      } catch {
        // Non-fatal: skip size if stat fails.
      }

      setBusy(true);
      const i = await analyzeApk(picked);
      setInfo(i);
      toast.success(`${i.package_name} (${i.version_name ?? '?'})`);

      // After baseline analysis succeeds, automatically run rule analysis if any
      // rule packs are installed. Failures here are non-fatal — the basic info
      // is still useful without rule evaluation.
      try {
        const sets = await listRules();
        setRuleSets(sets);
        if (sets.length > 0) {
          setRulesBusy(true);
          try {
            const report = await analyzeWithRules(picked, sets.map((s) => s.id));
            setRuleReport(report);
          } finally {
            setRulesBusy(false);
          }
        }
      } catch {
        // listRules / analyzeWithRules failures don't block the basic flow.
      }
    } catch (e) {
      const msg = String(e);
      setError(msg);
      toast.error(msg);
    } finally {
      setBusy(false);
    }
  }

  async function exportReport() {
    if (!info) return;
    const fileName = `${info.package_name}_report.html`;
    let dest: string | null = null;
    try {
      const picked = await saveDialog({
        defaultPath: fileName,
        filters: [{ name: 'HTML Report', extensions: ['html'] }],
      });
      dest = typeof picked === 'string' ? picked : null;
    } catch (e) {
      toast.error(String(e));
      return;
    }
    if (!dest) return;
    const html = buildReportHtml({
      apkInfo: info,
      ruleReport,
      generatedAt: new Date().toISOString(),
    });
    try {
      const r = await exportApkReport({ dest_path: dest, html });
      toast.success(
        t('analyze.exportSuccess', { path: r.dest_path, kb: Math.max(1, Math.round(r.bytes_written / 1024)) }),
      );
    } catch (e) {
      toast.error(t('analyze.exportFailed', { error: String(e) }));
    }
  }

  async function rerunRules() {
    if (!path || ruleSets.length === 0) return;
    setRulesBusy(true);
    try {
      const report = await analyzeWithRules(path, ruleSets.map((s) => s.id));
      setRuleReport(report);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setRulesBusy(false);
    }
  }

  const fileName = path ? path.split(/[\\/]/).pop() ?? path : null;
  const showAaptHint = error != null && isToolMissingAapt2(error);

  return (
    <div className="space-y-6 p-8">
      {/* Top bar — mirrors VSKiller's action row + file chip */}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold text-text-0">
            {t('nav.analyze')}
          </h1>
          <p className="text-sm text-text-2">
            {info ? t('analyze.appHeaderPickHint') : t('analyze.pickButton')}
          </p>
        </div>
        <div className="flex items-center gap-2">
          {path && (
            <Button
              variant="outline"
              size="sm"
              onClick={() => void rerunRules()}
              disabled={rulesBusy || ruleSets.length === 0}
            >
              {rulesBusy ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <RefreshCw className="h-4 w-4" />
              )}
              {t('analyze.rerunRules')}
            </Button>
          )}
          {info && (
            <Button
              variant="outline"
              size="sm"
              onClick={() => void exportReport()}
            >
              <Download className="h-4 w-4" />
              {t('analyze.exportReport')}
            </Button>
          )}
          <Button onClick={pickAndAnalyze} disabled={busy}>
            {busy ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <FolderOpen className="h-4 w-4" />
            )}
            {busy ? t('analyze.analyzing') : t('analyze.pickButton')}
          </Button>
        </div>
      </div>

      {/* Selected file chip — fades in after first pick */}
      {path && (
        <div
          className={cn(
            'flex flex-wrap items-center gap-2 rounded-xl border border-border bg-bg-1 px-4 py-2.5 text-xs text-text-2 shadow-card anim-fade',
          )}
        >
          <Sparkles className="h-3.5 w-3.5 text-brand" />
          <span className="font-mono truncate" title={path}>
            {fileName}
          </span>
          {size != null && <Badge variant="outline">{formatBytes(size)}</Badge>}
          {info && (
            <Badge variant="success">
              {info.application_label?.trim() || info.package_name}
            </Badge>
          )}
          {ruleSets.length > 0 && (
            <Badge variant="secondary">
              {ruleSets.length} {t('analyze.dashboardRules').toLowerCase()}
            </Badge>
          )}
        </div>
      )}

      {/* Working state */}
      {busy && (
        <Card>
          <CardContent className="flex items-center gap-3 text-sm text-text-1">
            <Loader2 className="h-4 w-4 animate-spin text-brand" />
            <span>
              {t('analyze.analyzingFile', {
                name: fileName ?? '',
                size: size != null ? formatBytes(size) : '—',
              })}
            </span>
          </CardContent>
        </Card>
      )}

      {/* Error */}
      {error && !busy && (
        <Card className="border-danger/60">
          <CardContent className="space-y-3 text-sm">
            <div className="flex items-start gap-3">
              <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-danger" />
              <div className="min-w-0 space-y-1">
                <div className="font-semibold text-text-0">
                  {t('analyze.errorTitle')}
                </div>
                <div className="break-all font-mono text-xs text-text-2">
                  {error}
                </div>
                {showAaptHint && (
                  <div className="pt-1 text-text-1">
                    {aaptPath
                      ? t('analyze.aaptPathSetButFailing', { path: aaptPath })
                      : t('analyze.aaptMissingHint')}
                  </div>
                )}
              </div>
            </div>
            <div className="flex justify-end gap-2">
              <Button
                variant="ghost"
                size="sm"
                onClick={() =>
                  getLogPath()
                    .then((p) => openPath(p))
                    .catch((e) => toast.error(String(e)))
                }
              >
                <ScrollText className="h-4 w-4" />
                {t('analyze.viewLog')}
              </Button>
              {showAaptHint && (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => navigate('/settings')}
                >
                  <SettingsIcon className="h-4 w-4" />
                  {t('analyze.openSettingsTools')}
                </Button>
              )}
            </div>
          </CardContent>
        </Card>
      )}

      {/* Hint when no rule packs */}
      {info && ruleSets.length === 0 && rulesPath && !rulesBusy && (
        <Card>
          <CardContent className="flex items-center justify-between gap-3 text-sm">
            <div className="flex items-center gap-2 text-text-2">
              <FileSearch className="h-4 w-4" />
              {t('analyze.noRulePacks')}
            </div>
            <Button
              variant="outline"
              size="sm"
              onClick={() => navigate('/settings')}
            >
              {t('analyze.openSettingsTools')}
            </Button>
          </CardContent>
        </Card>
      )}

      {/* Rules running indicator */}
      {rulesBusy && !ruleReport && (
        <Card>
          <CardContent className="flex items-center gap-3 text-sm text-text-1">
            <Loader2 className="h-4 w-4 animate-spin text-brand" />
            <span>{t('analyze.rulesRunning', { count: ruleSets.length })}</span>
          </CardContent>
        </Card>
      )}

      {/* The full report */}
      {info && <ApkInfoCard info={info} ruleReport={ruleReport} />}
    </div>
  );
}
