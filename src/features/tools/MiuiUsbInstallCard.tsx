import { useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import type { TFunction } from 'i18next';
import '@/i18n';
import {
  AlertTriangle,
  Check,
  ChevronDown,
  ChevronUp,
  Hammer,
  Loader2,
  Play,
  RotateCcw,
} from 'lucide-react';
import { toast } from 'sonner';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { useStepRunner, type Step } from './_shared/stepRunner';
import { safeShell, quoteForSu } from './_shared/safeShell';
import { StepRunnerDetails } from './_shared/StepRunnerDetails';

/** Path to the MIUI securitycenter prefs file this tool rewrites. Kept
 *  as a constant so future tools (e.g. HyperOS variants) can swap it
 *  without copy-pasting. */
const MIUI_PREFS_FILE =
  '/data/data/com.miui.securitycenter/shared_prefs/remote_provider_preferences.xml';

/** Read current value of a `persist.*` property. Returns the raw
 *  trimmed stdout of `getprop`, or `null` if the call fails. */
async function readProp(
  serial: string,
  name: string,
  log: (line: string) => void,
): Promise<string | null> {
  const out = await safeShell(serial, `getprop ${name}`);
  if (!out) {
    log(`[prop] getprop ${name} failed`);
    return null;
  }
  return out.stdout.trim();
}

/** Insert <boolean name="..." value="..." /> before the first </map>
 *  tag (or, fallback, at line 3) so the XML stays well-formed for
 *  SharedPreferences. */
function insertBoolean(xml: string, name: string, value: string): string {
  const line = `    <boolean name="${name}" value="${value}" />`;
  const closeIdx = xml.indexOf('</map>');
  if (closeIdx >= 0) {
    return xml.slice(0, closeIdx) + line + '\n' + xml.slice(closeIdx);
  }
  const lines = xml.split('\n');
  lines.splice(Math.min(2, lines.length), 0, line);
  return lines.join('\n');
}

/** Write `xml` back to the prefs file via `su`. Uses base64 to
 *  keep the transport hermetic -- heredocs and quote escapes are
 *  fragile in the device's `sh` parser when the value contains
 *  mixed quote / newline characters. `base64 -d` ships with
 *  toybox on every modern Android build (10+). */
async function writeBack(
  serial: string,
  xml: string,
  log: (line: string) => void,
): Promise<{ ok: boolean; skipped?: boolean; detail?: string }> {
  // Encode as base64 in the renderer. `btoa` throws on non-Latin1
  // characters; XML from SharedPreferences is always ASCII so the
  // exception path won't trigger in practice, but fall back to a
  // regular write if it does (the rare device with broken base64 will
  // surface it as a write-failure rather than a hang).
  let b64: string;
  try {
    b64 = btoa(unescape(encodeURIComponent(xml)));
  } catch (e) {
    log(`[writeBack] btoa failed: ${String(e)}`);
    return { ok: false, detail: 'btoa failed' };
  }
  const inner = `echo '${b64}' | base64 -d > "${MIUI_PREFS_FILE}"`;
  const cmd = `su -c ${quoteForSu(inner)}`;
  log(`[writeBack] writing ${b64.length}b base64 -> ${MIUI_PREFS_FILE}`);
  const out = await safeShell(serial, cmd);
  if (!out) {
    return { ok: false, detail: 'no reply' };
  }
  if (out.exitCode !== 0) {
    return { ok: false, detail: out.stderr.trim() || 'exit ' + out.exitCode };
  }
  return { ok: true };
}

/**
 * Define the steps for the MIUI USB-install script. Each step mirrors
 * one logical block of the original `enable_miui_adb.sh` but
 * expressed as an `adb shell` call so we get step-by-step progress
 * + per-step exit codes.
 */
function buildMiuiSteps(t: TFunction): Step[] {
  return [
    {
      id: 'checkRoot',
      labelKey: 'checkRoot',
      run: async (serial, log) => {
        const out = await safeShell(serial, "su -c 'id'");
        if (!out) {
          throw new Error(t('tools.miuiUsbInstall.rootFailed', { error: 'no su reply' }));
        }
        if (!/uid=0/.test(out.stdout)) {
          throw new Error(
            t('tools.miuiUsbInstall.rootFailed', { error: out.stdout.trim() || out.stderr.trim() || 'not root' }),
          );
        }
        log(`[checkRoot] ${out.stdout.trim()}`);
        log(t('tools.miuiUsbInstall.rootOk'));
        return { ok: true };
      },
    },
    {
      id: 'adbInputProp',
      labelKey: 'adbInputProp',
      run: async (serial, log) => {
        const cur = await readProp(serial, 'persist.security.adbinput', log);
        if (cur === '1') {
          log(t('tools.miuiUsbInstall.propsUnchanged'));
          return { ok: true, skipped: true };
        }
        const out = await safeShell(
          serial,
          `su -c ${quoteForSu('setprop persist.security.adbinput 1')}`,
        );
        if (!out || out.exitCode !== 0) {
          throw new Error(t('tools.runFailed', {
            error: out?.stderr.trim() || 'setprop failed',
          }));
        }
        log(t('tools.miuiUsbInstall.propsSet'));
        return { ok: true, detail: 'persist.security.adbinput=1' };
      },
    },
    {
      id: 'fastbootProp',
      labelKey: 'fastbootProp',
      run: async (serial, log) => {
        const cur = await readProp(serial, 'persist.fastboot.enable', log);
        if (cur === '1') {
          log(t('tools.miuiUsbInstall.propsUnchanged'));
          return { ok: true, skipped: true };
        }
        const out = await safeShell(
          serial,
          `su -c ${quoteForSu('setprop persist.fastboot.enable 1')}`,
        );
        if (!out || out.exitCode !== 0) {
          throw new Error(t('tools.runFailed', {
            error: out?.stderr.trim() || 'setprop failed',
          }));
        }
        log(t('tools.miuiUsbInstall.propsSet'));
        return { ok: true, detail: 'persist.fastboot.enable=1' };
      },
    },
    {
      id: 'modifyXml',
      labelKey: 'modifyXml',
      run: async (serial, log) => {
        // Read the prefs file via su. Some MIUI versions leave it empty
        // until securitycenter has been launched at least once -- in that
        // case the file just won't exist and we have nothing to amend.
        const read = await safeShell(
          serial,
          `su -c ${quoteForSu(`cat "${MIUI_PREFS_FILE}"`)}`,
        );
        if (!read) {
          throw new Error('cat prefs failed');
        }
        if (read.exitCode !== 0) {
          // File probably doesn't exist -- bootstrap with the two keys
          // we care about and write it back so subsequent runs are
          // idempotent.
          if (/No such file/i.test(read.stderr) || read.stdout.trim() === '') {
            log('[modifyXml] prefs file missing, bootstrapping');
            const fresh =
              '<?xml version=\'1.0\' encoding=\'utf-8\' standalone=\'yes\' ?>\n' +
              '<map>\n' +
              '    <boolean name="security_adb_install_enable" value="true" />\n' +
              '    <boolean name="permcenter_install_intercept_enabled" value="false" />\n' +
              '</map>\n';
            return await writeBack(serial, fresh, log);
          }
          throw new Error(read.stderr.trim() || 'cat prefs failed');
        }

        let xml = read.stdout;
        let touched = false;

        // security_adb_install_enable: false -> true; insert if absent.
        if (/<boolean\s+name="security_adb_install_enable"\s+value="false"\s*\/>/.test(xml)) {
          xml = xml.replace(
            /<boolean\s+name="security_adb_install_enable"\s+value="false"\s*\/>/,
            '<boolean name="security_adb_install_enable" value="true" />',
          );
          touched = true;
        } else if (!/<boolean\s+name="security_adb_install_enable"/.test(xml)) {
          xml = insertBoolean(xml, 'security_adb_install_enable', 'true');
          touched = true;
        }

        // permcenter_install_intercept_enabled: true -> false; insert if absent.
        if (/<boolean\s+name="permcenter_install_intercept_enabled"\s+value="true"\s*\/>/.test(xml)) {
          xml = xml.replace(
            /<boolean\s+name="permcenter_install_intercept_enabled"\s+value="true"\s*\/>/,
            '<boolean name="permcenter_install_intercept_enabled" value="false" />',
          );
          touched = true;
        } else if (!/<boolean\s+name="permcenter_install_intercept_enabled"/.test(xml)) {
          xml = insertBoolean(xml, 'permcenter_install_intercept_enabled', 'false');
          touched = true;
        }

        if (!touched) {
          log('[modifyXml] already enabled');
          return { ok: true, skipped: true };
        }
        return await writeBack(serial, xml, log);
      },
    },
    {
      id: 'killCenter',
      labelKey: 'killCenter',
      optional: true,
      run: async (serial, log) => {
        const out = await safeShell(
          serial,
          `su -c ${quoteForSu('kill -9 $(pidof com.miui.securitycenter.remote)')}`,
        );
        if (!out) {
          log('[kill] no reply');
          return { ok: true, skipped: true };
        }
        if (out.exitCode !== 0 && out.stderr.trim()) {
          // Non-fatal: a "no such process" / pidof-empty exit isn't
          // worth surfacing as a failure.
          log(`[kill] stderr: ${out.stderr.trim()}`);
        }
        log('[kill] com.miui.securitycenter.remote killed');
        return { ok: true };
      },
    },
  ];
}

export function MiuiUsbInstallCard({ serial }: { serial: string }) {
  const { t } = useTranslation();
  // `steps` is built once per locale change. The closures capture
  // `t` so labels show current translations even if the user
  // switches languages mid-run -- the visible status array is
  // recomputed when the user clicks run again, but the captured
  // function references keep working without a re-build.
  const stepsRef = useRef<Step[]>(buildMiuiSteps(t));
  useEffect(() => {
    stepsRef.current = buildMiuiSteps(t);
  }, [t]);
  const steps = stepsRef.current;

  const runner = useStepRunner(steps, serial);

  async function run() {
    const intro = `[start] ${t('tools.miuiUsbInstall.title')} @ ${serial}`;
    const ok = await runner.run(intro);
    if (ok) {
      toast.success(t('tools.miuiUsbInstall.success'));
    } else {
      toast.error(t('tools.runFailed', { error: t('tools.miuiUsbInstall.failTitle') }));
    }
  }

  const completedCount = runner.rows.filter((r) => r.status !== 'pending').length;

  return (
    <Card className="overflow-hidden">
      <CardContent className="p-3">
        {/* Single-row compact layout: [icon] [title+subtitle stack] [actions]. */}
        <div className="flex items-center gap-2.5">
          <div className="grid h-8 w-8 shrink-0 place-items-center rounded-md bg-warning/15 text-warning">
            <Hammer className="h-4 w-4" />
          </div>

          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-1.5">
              <h3 className="truncate text-sm font-medium leading-tight text-text-0">
                {t('tools.miuiUsbInstall.title')}
              </h3>
              <Badge
                variant="warning"
                className="h-4 shrink-0 px-1 py-0 text-[10px] leading-none"
              >
                root
              </Badge>
              {runner.lastResult === 'success' && (
                <span
                  className="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full bg-success/15 text-success"
                  aria-label={t('tools.miuiUsbInstall.resultSuccess')}
                  title={t('tools.miuiUsbInstall.resultSuccess')}
                >
                  <Check className="h-3 w-3" />
                </span>
              )}
              {runner.lastResult === 'failed' && (
                <span
                  className="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full bg-danger/15 text-danger"
                  aria-label={t('tools.miuiUsbInstall.resultFailed')}
                  title={t('tools.miuiUsbInstall.resultFailed')}
                >
                  <AlertTriangle className="h-3 w-3" />
                </span>
              )}
              {runner.running && (
                <span className="inline-flex shrink-0 items-center gap-0.5 text-[10px] text-text-2">
                  <Loader2 className="h-3 w-3 animate-spin" />
                  {completedCount}/{runner.rows.length}
                </span>
              )}
            </div>
            <p className="truncate text-[11px] leading-tight text-text-2">
              {t('tools.miuiUsbInstall.subtitle')}
            </p>
          </div>

          <div className="flex shrink-0 items-center gap-1">
            {runner.hasDetails && (
              <button
                type="button"
                onClick={() => runner.setDetailsOpen((v) => !v)}
                aria-expanded={runner.detailsOpen}
                aria-label={
                  runner.detailsOpen
                    ? t('tools.miuiUsbInstall.hideDetails')
                    : t('tools.miuiUsbInstall.showDetails')
                }
                className="grid h-7 w-7 place-items-center rounded-md text-text-2 transition-colors hover:bg-bg-2 hover:text-text-0"
              >
                {runner.detailsOpen ? (
                  <ChevronUp className="h-3.5 w-3.5" />
                ) : (
                  <ChevronDown className="h-3.5 w-3.5" />
                )}
              </button>
            )}
            <Button
              onClick={() => void run()}
              disabled={runner.running}
              size="sm"
              className="h-7 gap-1 px-2.5"
            >
              {runner.running ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : runner.lastResult !== null ? (
                <RotateCcw className="h-3.5 w-3.5" />
              ) : (
                <Play className="h-3.5 w-3.5" />
              )}
              {runner.running
                ? t('tools.miuiUsbInstall.running')
                : runner.lastResult !== null
                ? t('tools.miuiUsbInstall.retry')
                : t('tools.miuiUsbInstall.run')}
            </Button>
          </div>
        </div>

        {runner.detailsOpen && runner.hasDetails && (
          <StepRunnerDetails
            rows={runner.rows}
            logLines={runner.logLines}
            labelNamespace="tools.miuiUsbInstall.steps"
            outputTitleLabel={t('tools.miuiUsbInstall.outputTitle')}
            copyLogLabel={t('tools.miuiUsbInstall.copyLog')}
            logCopiedLabel={t('tools.miuiUsbInstall.logCopied')}
          />
        )}
      </CardContent>
    </Card>
  );
}
