import { useTranslation } from 'react-i18next';
import { AlertTriangle, Check, Circle, Loader2, Minus } from 'lucide-react';
import { cn } from '@/lib/utils';
import type { StepRow } from './stepRunner';

/**
 * Compact one-line row: status icon (fixed 16px slot) + label + optional
 * inline error detail. The legacy variant wrapped each row in its own
 * bordered card with a 48px-wide Badge; trimming those lets 5 steps
 * stack into ~80px of vertical space when expanded, which keeps the
 * collapsible details panel proportional to the compact card body.
 *
 * `labelKey` is the suffix under the tool's namespace; the caller
 * supplies `labelNamespace` so this stays generic across
 * `tools.miuiUsbInstall.steps.*`, `tools.devOptions.steps.*`, etc.
 */
export function StepRowView({
  row,
  labelNamespace,
}: {
  row: StepRow;
  labelNamespace: string;
}) {
  const { t } = useTranslation();
  const labelKey = `${labelNamespace}.${row.labelKey}`;
  type Meta = { icon: React.ReactNode; cls: string };
  const meta: Meta = (() => {
    switch (row.status) {
      case 'running':
        return { icon: <Loader2 className="h-3 w-3 animate-spin" />, cls: 'text-warning' };
      case 'success':
        return { icon: <Check className="h-3 w-3" />, cls: 'text-success' };
      case 'skipped':
        return { icon: <Minus className="h-3 w-3" />, cls: 'text-text-2' };
      case 'error':
        return { icon: <AlertTriangle className="h-3 w-3" />, cls: 'text-danger' };
      case 'pending':
      default:
        return { icon: <Circle className="h-3 w-3" />, cls: 'text-text-2' };
    }
  })();
  return (
    <div className="flex items-center gap-2 py-0.5 text-xs">
      <span className={cn('flex w-4 shrink-0 justify-center', meta.cls)}>
        {meta.icon}
      </span>
      <span className="flex-1 text-text-1">{t(labelKey)}</span>
      {row.status === 'error' && row.detail && (
        <span
          className="ml-2 max-w-[40%] truncate font-mono text-danger"
          title={row.detail}
        >
          {row.detail}
        </span>
      )}
    </div>
  );
}
