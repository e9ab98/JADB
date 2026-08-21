import { useEffect, useState } from 'react';
import { getAppVersion, type AppVersionInfo } from '@/ipc/system';

/**
 * Subscribe to the application version. The hook:
 *   * Calls `get_app_version` once on mount.
 *   * Caches the result in React state so multiple Sidebar / About
 *     panes share one fetch.
 *   * Falls back to `null` when the call fails (e.g. a dev build
 *     where the command is not registered yet). Callers must
 *     trivially degrade — the typical use is `<span>v{info?.version ?? '—'}</span>`.
 *
 * The `version` field is the same string we want to render in:
 *   - The Sidebar footer.
 *   - The "About" / settings panel.
 *   - The title screen on the splash.
 *
 * No polling: the version is baked into the binary at build time
 * and never changes within a single run.
 */
export function useAppVersion(): AppVersionInfo | null {
  const [info, setInfo] = useState<AppVersionInfo | null>(null);
  useEffect(() => {
    let cancelled = false;
    getAppVersion()
      .then((value) => {
        if (!cancelled) setInfo(value);
      })
      .catch(() => {
        // Best-effort: we already draw a "—" fallback in the UI
        // for the very small chance this fails (e.g. the command
        // is missing from an older build during hot reload).
        if (!cancelled) setInfo(null);
      });
    return () => {
      cancelled = true;
    };
  }, []);
  return info;
}
