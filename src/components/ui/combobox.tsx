import * as React from 'react';
import { Check, ChevronDown, Search, X } from 'lucide-react';
import { cn } from '@/lib/utils';

type ComboboxProps = {
  /** Currently selected option. Empty string = nothing selected. */
  value: string;
  onChange: (value: string) => void;
  /** All selectable options. The combobox does NOT sort them — caller
   *  decides (so we can show installed apps in the order `pm list
   *  packages` returns them). */
  options: string[];
  /** Shown when nothing is selected. */
  placeholder?: string;
  /** Shown in the empty-state row when the user's filter matches no
   *  options. */
  emptyText?: string;
  /** Label for the "no selection" row that gets prepended when this is
   *  true (e.g. "All packages" for a filter dropdown). */
  allowEmpty?: boolean;
  emptyLabel?: string;
  /** Placeholder for the inline filter input inside the popover. */
  filterPlaceholder?: string;
  /** `true` while the list is being fetched — disables the trigger. */
  loading?: boolean;
  disabled?: boolean;
  /** Extra className applied to the outer wrapper. */
  className?: string;
};

/**
 * Lightweight combobox: trigger button + popover with an inline filter
 * input. Built without `cmdk` / `react-popover` to keep the dep tree
 * minimal — Radix DropdownMenu is the only Radix primitive we already
 * pull, but it's built for menu semantics, so we roll our own
 * positioning + click-outside for a search-first dropdown.
 */
export function Combobox({
  value,
  onChange,
  options,
  placeholder,
  emptyText,
  allowEmpty,
  emptyLabel,
  filterPlaceholder,
  loading,
  disabled,
  className,
}: ComboboxProps) {
  const [open, setOpen] = React.useState(false);
  const [query, setQuery] = React.useState('');
  const rootRef = React.useRef<HTMLDivElement>(null);

  React.useEffect(() => {
    if (!open) return;
    const onDocMouseDown = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', onDocMouseDown);
    return () => document.removeEventListener('mousedown', onDocMouseDown);
  }, [open]);

  // Reset the filter whenever the popover opens so users see the full
  // list (their previous typing is gone, but their selection is kept).
  React.useEffect(() => {
    if (open) setQuery('');
  }, [open]);

  const q = query.trim().toLowerCase();
  const filtered = q
    ? options.filter((o) => o.toLowerCase().includes(q))
    : options;

  const select = (next: string) => {
    onChange(next);
    setOpen(false);
  };

  return (
    <div ref={rootRef} className={cn('relative', className)}>
      <button
        type="button"
        onClick={() => !disabled && !loading && setOpen((o) => !o)}
        disabled={disabled || loading}
        className={cn(
          'flex h-9 w-full items-center justify-between gap-2 rounded-md border border-border bg-bg-1 px-3 py-1 text-left text-sm text-text-0',
          'transition-colors hover:bg-bg-2 focus:outline-none focus:ring-2 focus:ring-brand',
          (disabled || loading) && 'cursor-not-allowed opacity-50',
        )}
      >
        <span className={cn('truncate', !value && 'text-text-2')}>
          {value || placeholder}
        </span>
        <div className="flex shrink-0 items-center gap-1">
          {value && !disabled && !loading && (
            <X
              className="h-3.5 w-3.5 text-text-2 hover:text-text-0"
              onClick={(e) => {
                e.stopPropagation();
                onChange('');
              }}
            />
          )}
          <ChevronDown
            className={cn(
              'h-3.5 w-3.5 text-text-2 transition-transform',
              open && 'rotate-180',
            )}
          />
        </div>
      </button>

      {open && (
        <div className="absolute left-0 right-0 top-full z-50 mt-1 overflow-hidden rounded-md border border-border bg-bg-0 shadow-card">
          <div className="flex items-center gap-2 border-b border-border bg-bg-1 px-2 py-1.5">
            <Search className="h-3.5 w-3.5 shrink-0 text-text-2" />
            <input
              autoFocus
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={filterPlaceholder}
              className="h-7 w-full bg-transparent text-sm text-text-0 placeholder:text-text-2 focus:outline-none"
            />
          </div>
          <ul className="max-h-64 overflow-auto py-1">
            {allowEmpty && (
              <li>
                <button
                  type="button"
                  className={cn(
                    'flex w-full items-center justify-between gap-2 px-3 py-1.5 text-left text-sm text-text-1 hover:bg-bg-2',
                    !value && 'bg-bg-2 font-medium text-text-0',
                  )}
                  onClick={() => select('')}
                >
                  <span className="truncate">{emptyLabel}</span>
                  {!value && <Check className="h-3.5 w-3.5 shrink-0 text-brand" />}
                </button>
              </li>
            )}
            {filtered.length === 0 ? (
              <li className="px-3 py-3 text-center text-xs text-text-2">
                {emptyText}
              </li>
            ) : (
              filtered.map((opt) => {
                const selected = opt === value;
                return (
                  <li key={opt}>
                    <button
                      type="button"
                      className={cn(
                        'flex w-full items-center justify-between gap-2 px-3 py-1.5 text-left text-sm hover:bg-bg-2',
                        selected && 'bg-bg-2 font-medium text-text-0',
                      )}
                      onClick={() => select(opt)}
                    >
                      <span className="truncate font-mono text-xs">{opt}</span>
                      {selected && (
                        <Check className="h-3.5 w-3.5 shrink-0 text-brand" />
                      )}
                    </button>
                  </li>
                );
              })
            )}
          </ul>
        </div>
      )}
    </div>
  );
}
