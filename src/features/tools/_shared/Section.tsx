import { useState } from 'react';
import { ChevronDown, ChevronUp } from 'lucide-react';
import { cn } from '@/lib/utils';

/**
 * A labelled group of tool cards.
 *
 * Sections give the (otherwise-uncategorised) Tools tab a visual
 * hierarchy: instead of N cards stacked into a single grid, the
 * user sees `Connection` -> `Device Actions` -> `App Management`
 * blocks and can collapse the ones they don't need right now.
 *
 * Header layout (single row, matches the panel's header style):
 *   [icon] Title                      [count badge?]  [chevron]
 *          Subtitle
 *   ───────────────────────────────────────────────
 *
 * Default state: expanded. `defaultOpen={false}` flips it.
 * Collapsed state hides the children; the user gets a one-row
 * band per section instead of nothing.
 */
export function Section({
  title,
  subtitle,
  defaultOpen = true,
  children,
}: {
  title: string;
  subtitle?: string;
  defaultOpen?: boolean;
  children: React.ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <section className="space-y-2">
      <header className="flex items-center gap-2 border-b border-border pb-1">
        <h2 className="text-[13px] font-semibold leading-tight text-text-0">
          {title}
        </h2>
        {subtitle && (
          <span className="text-[11px] leading-tight text-text-2">
            {subtitle}
          </span>
        )}
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          aria-expanded={open}
          aria-label={open ? `${title} collapse` : `${title} expand`}
          className={cn(
            'ml-auto grid h-6 w-6 shrink-0 place-items-center rounded-md',
            'text-text-2 transition-colors hover:bg-bg-2 hover:text-text-0',
          )}
        >
          {open ? (
            <ChevronUp className="h-3.5 w-3.5" />
          ) : (
            <ChevronDown className="h-3.5 w-3.5" />
          )}
        </button>
      </header>
      {open && <div className="space-y-4">{children}</div>}
    </section>
  );
}
