import { useEffect } from 'react';
import { createElement } from 'react';
import { toast } from 'sonner';
import { checkForUpdate, wasDismissed, shouldNotify, LS_KEYS } from '@/lib/update';
import { UpdateToast } from '@/features/updateNotification/UpdateToast';

const UPDATE_TOAST_ID = 'jadb-update';

function wasCheckedWithin24h(): boolean {
  const raw = localStorage.getItem(LS_KEYS.checked);
  if (!raw) return false;
  const t = Date.parse(raw);
  if (Number.isNaN(t)) return false;
  return Date.now() - t < 24 * 60 * 60 * 1000;
}

export function useUpdateCheck(): void {
  useEffect(() => {
    // The user can disable automatic update checks in Settings → Updates.
    if (!shouldNotify()) return;
    // Don't burn the user's network on every app launch; throttle to
    // one check per 24h. The Sidebar's manual button bypasses this.
    if (wasCheckedWithin24h()) return;

    let cancelled = false;

    // Defer the check by 5 seconds so app startup / first-paint is
    // completely unaffected. Set a 5s network timeout so a hanging
    // endpoint fails fast.
    const timer = window.setTimeout(() => {
      checkForUpdate({ timeout: 5000 })
        .then((update) => {
          if (cancelled) return;
          if (!update) return;
          if (wasDismissed(update.version)) return;
          toast(
            createElement(UpdateToast, {
              version: update.version,
              notes: update.notes,
            }),
            { id: UPDATE_TOAST_ID, duration: Infinity },
          );
        })
        .catch(() => {
          // Update checks are best-effort and must not affect application startup.
        });
    }, 5000);

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, []);
}
