import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import {
  CheckCircle2,
  Eraser,
  Loader2,
  Play,
  Smartphone,
  TerminalSquare,
  XCircle,
} from 'lucide-react';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { adbShell, type ShellOutput } from '@/ipc/adb';
import { cn } from '@/lib/utils';

type Props = {
  serial: string | null;
};

type Run = {
  id: number;
  command: string;
  output: ShellOutput | null;
  startedAt: number;
  finishedAt: number | null;
  error: string | null;
};

const QUICK_COMMANDS = [
  'pm list packages -3',
  'pm list packages',
  'dumpsys activity activities | grep mResumedActivity',
  'getprop ro.build.version.release',
  'getprop ro.product.model',
  'echo hello',
];

export function AdbShellTab({ serial }: Props) {
  const { t } = useTranslation();
  const [command, setCommand] = useState('');
  const [busy, setBusy] = useState(false);
  const [runs, setRuns] = useState<Run[]>([]);
  const idRef = useRef(0);
  const scrollRef = useRef<HTMLDivElement | null>(null);

  // Auto-scroll the output panel to the bottom when new runs finish.
  useEffect(() => {
    if (!scrollRef.current) return;
    const viewport = scrollRef.current.querySelector(
      '[data-radix-scroll-area-viewport]',
    ) as HTMLDivElement | null;
    if (viewport) viewport.scrollTop = viewport.scrollHeight;
  }, [runs]);

  async function run(rawCommand?: string) {
    if (!serial) return;
    const cmd = (rawCommand ?? command).trim();
    if (!cmd) return;
    setBusy(true);
    const id = ++idRef.current;
    const run: Run = {
      id,
      command: cmd,
      output: null,
      startedAt: Date.now(),
      finishedAt: null,
      error: null,
    };
    setRuns((prev) => [...prev, run]);
    setCommand('');
    try {
      const out = await adbShell(serial, cmd);
      setRuns((prev) =>
        prev.map((r) =>
          r.id === id
            ? { ...r, output: out, finishedAt: Date.now() }
            : r,
        ),
      );
    } catch (e) {
      setRuns((prev) =>
        prev.map((r) =>
          r.id === id
            ? { ...r, error: String(e), finishedAt: Date.now() }
            : r,
        ),
      );
    } finally {
      setBusy(false);
    }
  }

  function onKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void run();
    }
  }

  function clear() {
    setRuns([]);
  }

  if (!serial) {
    return (
      <Card>
        <CardContent className="flex items-center gap-3 text-sm text-text-2">
          <Smartphone className="h-4 w-4" />
          {t('adb.noDeviceSelected')}
        </CardContent>
      </Card>
    );
  }

  return (
    <div className="space-y-4">
      <Card>
        <CardContent className="space-y-3">
          <div className="flex items-center justify-between gap-2">
            <div className="flex items-center gap-2 text-sm font-medium text-text-0">
              <TerminalSquare className="h-4 w-4 text-text-1" />
              {t('adb.shellTitle')}
            </div>
            <div className="flex items-center gap-2 text-xs text-text-2">
              <Badge variant="outline">{serial}</Badge>
              <span>·</span>
              <span>
                {t('adb.shellRunCount', { count: runs.length })}
              </span>
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <Input
              value={command}
              onChange={(e) => setCommand(e.target.value)}
              onKeyDown={onKeyDown}
              placeholder={t('adb.shellPlaceholder')}
              disabled={busy}
              className="flex-1 min-w-[200px] font-mono"
              autoComplete="off"
              spellCheck={false}
            />
            <Button onClick={() => void run()} disabled={busy || !command.trim()}>
              {busy ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Play className="h-4 w-4" />
              )}
              {t('adb.shellRun')}
            </Button>
            <Button
              variant="outline"
              onClick={clear}
              disabled={busy || runs.length === 0}
            >
              <Eraser className="h-4 w-4" />
              {t('adb.shellClear')}
            </Button>
          </div>

          <div className="flex flex-wrap items-center gap-1">
            <span className="text-xs text-text-2">{t('adb.shellQuick')}</span>
            {QUICK_COMMANDS.map((q) => (
              <button
                key={q}
                type="button"
                onClick={() => {
                  setCommand(q);
                  void run(q);
                }}
                disabled={busy}
                className={cn(
                  'rounded-md border border-border bg-bg-1 px-2 py-0.5 font-mono text-xs text-text-1 transition-colors',
                  'hover:border-border-strong hover:bg-bg-2 disabled:opacity-50',
                )}
                title={q}
              >
                {q}
              </button>
            ))}
          </div>
        </CardContent>
      </Card>

      <Card className="overflow-hidden">
        <CardContent className="p-0">
          {runs.length === 0 ? (
            <div className="px-4 py-6 text-sm text-text-2">
              {t('adb.shellEmpty')}
            </div>
          ) : (
            <ScrollArea ref={scrollRef} className="max-h-[60vh]">
              <div className="divide-y divide-border">
                {runs.map((r) => (
                  <RunBlock key={r.id} run={r} />
                ))}
              </div>
            </ScrollArea>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function RunBlock({ run }: { run: Run }) {
  const { t } = useTranslation();
  const durationMs = (run.finishedAt ?? Date.now()) - run.startedAt;
  const stdout = run.output?.stdout ?? '';
  const stderr = run.output?.stderr ?? '';
  const exitCode = run.output?.exitCode;
  const exitTone =
    run.error !== null
      ? 'danger'
      : exitCode === 0
      ? 'success'
      : exitCode !== undefined
      ? 'warning'
      : 'secondary';

  return (
    <div className="px-4 py-3">
      <div className="mb-2 flex items-center gap-2 text-xs">
        <Badge variant={exitTone}>
          {run.error
            ? t('adb.shellFailed')
            : exitCode === undefined
            ? t('adb.shellRunning')
            : exitCode === 0
            ? t('adb.shellExit0')
            : t('adb.shellExitCode', { code: exitCode })}
        </Badge>
        <span className="font-mono text-text-1">{run.command}</span>
        <span className="ml-auto text-text-2">
          {durationMs < 1000
            ? `${durationMs} ms`
            : `${(durationMs / 1000).toFixed(2)} s`}
        </span>
      </div>
      {run.error ? (
        <pre className="overflow-x-auto whitespace-pre-wrap break-all rounded-md bg-danger/5 p-2 font-mono text-xs text-danger">
          {run.error}
        </pre>
      ) : (
        <pre
          className={cn(
            'overflow-x-auto whitespace-pre-wrap break-all rounded-md bg-bg-2 p-2 font-mono text-xs',
            stderr ? 'text-warning' : 'text-text-1',
          )}
        >
          {(stdout || stderr) || (
            <span className="text-text-2">(no output)</span>
          )}
        </pre>
      )}
      {run.output && stderr && (
        <details className="mt-2 text-xs text-text-2">
          <summary className="cursor-pointer select-none text-warning">
            <XCircle className="mr-1 inline h-3 w-3" />
            {t('adb.shellStderr', { bytes: stderr.length })}
          </summary>
          <pre className="mt-1 overflow-x-auto whitespace-pre-wrap break-all rounded-md bg-bg-2 p-2 font-mono text-xs text-warning">
            {stderr}
          </pre>
        </details>
      )}
      {run.output && !stderr && stdout && (
        <div className="mt-1 flex items-center gap-1 text-xs text-success">
          <CheckCircle2 className="h-3 w-3" />
          {t('adb.shellStdout', { bytes: stdout.length })}
        </div>
      )}
    </div>
  );
}
