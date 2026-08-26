import { useTranslation } from 'react-i18next';
import { Clipboard, Terminal as TerminalIcon } from 'lucide-react';
import { toast } from 'sonner';
import { ScrollArea } from '@/components/ui/scroll-area';
import type { StepRow } from './stepRunner';
import { StepRowView } from './StepRowView';

/**
 * Collapsible details panel used by every step-driven tool card:
 *   1. A list of step rows (`StepRowView`).
 *   2. A scrollable log buffer with a "copy log" button.
 *
 * The header (icon, title, badges, run button, etc.) stays local to
 * each tool card because every tool has a slightly different shape
 * (root badge, duration input, multi-step wizard, ...). Everything
 * below the divider is genuinely identical, though, so it lives
 * here.
 */
export function StepRunnerDetails({
  rows,
  logLines,
  labelNamespace,
  outputTitleLabel,
  copyLogLabel,
  logCopiedLabel,
}: {
  rows: StepRow[];
  logLines: string[];
  labelNamespace: string;
  outputTitleLabel: string;
  copyLogLabel: string;
  logCopiedLabel: string;
}) {
  const { t } = useTranslation();

  function copyLog() {
    const text = logLines.join('\n');
    if (typeof navigator !== 'undefined' && navigator.clipboard) {
      void navigator.clipboard.writeText(text).then(
        () => toast.success(logCopiedLabel),
        () => {/* swallow */},
      );
    }
  }

  const showRows = rows.some((r) => r.status !== 'pending');
  const showLog = logLines.length > 0;

  if (!showRows && !showLog) return null;

  return (
    <div className="mt-3 space-y-2 border-t border-border pt-3">
      {showRows && (
        <div className="space-y-0.5">
          {rows.map((row) => (
            <StepRowView key={row.id} row={row} labelNamespace={labelNamespace} />
          ))}
        </div>
      )}
      {showLog && (
        <div className="rounded-md border border-border bg-bg-2">
          <div className="flex items-center justify-between border-b border-border px-2 py-1 text-[11px] text-text-2">
            <div className="inline-flex items-center gap-1">
              <TerminalIcon className="h-3 w-3" />
              {outputTitleLabel}
            </div>
            <button
              type="button"
              onClick={copyLog}
              className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-text-2 hover:bg-bg-1 hover:text-text-0"
              title={copyLogLabel}
            >
              <Clipboard className="h-3 w-3" />
              {t('tools.miuiUsbInstall.copyLog')}
            </button>
          </div>
          <ScrollArea className="max-h-56">
            <pre className="overflow-x-auto whitespace-pre-wrap break-all p-2 font-mono text-[11px] leading-relaxed text-text-1">
              {logLines.join('\n')}
            </pre>
          </ScrollArea>
        </div>
      )}
    </div>
  );
}
