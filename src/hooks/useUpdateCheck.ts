import { useEffect } from 'react';
import { createElement } from 'react';
import { toast } from 'sonner';
import { checkForUpdate } from '@/lib/update';
import { UpdateToast } from '@/features/updateNotification/UpdateToast';

const UPDATE_TOAST_ID = 'jadb-update';

export function useUpdateCheck(): void {
  useEffect(() => {
    const timer = window.setTimeout(() => {
      checkForUpdate({ timeout: 5000 }).then((update) => {
        if (!update) return;
        toast(createElement(UpdateToast, { version: update.version, notes: update.notes }), {
          id: UPDATE_TOAST_ID,
          duration: Infinity,
        });
      }).catch(() => {
        // Update checks are best-effort and must not affect application startup.
      });
    }, 5000);

    return () => window.clearTimeout(timer);
  }, []);
}
