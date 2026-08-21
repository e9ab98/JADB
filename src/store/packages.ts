import { create } from 'zustand';
import { adbListPackagesViaAgent, type AppInfo, type DeviceSystemInfo } from '@/ipc/adb';

/**
 * Per-device package cache, shared across every tab that needs the
 * installed-package list (Apps / Logcat capture / future consumers).
 * Fetched once per `serial` and reused until explicitly refreshed.
 *
 * What we cache:
 *   - The agent's full `AppInfo` per package (label, version, size,
 *     install paths, isSystem, isDebuggable, ...). Anything the agent
 *     emits lives here -- no per-package `dumpsys + pull + aapt2` calls.
 *   - Icons are NOT cached -- they're a few KB each and can be 50+ per
 *     app list. AdbAppsTab pulls icons on-demand per card.
 *
 * Lifecycle:
 *   - `ensureLoaded(serial)` is called by every consumer on mount. The
 *     store is idempotent: a fresh cache is reused, only the in-flight
 *     promise is shared across concurrent callers.
 *   - `refresh(serial)` forces a fresh fetch.
 *   - `drop(serial)` clears the cache for one device (e.g. on disconnect).
 *
 * Failure policy:
 *   - `adbListPackagesViaAgent` already has a Rust-side fallback (agent
 *     → `pm list packages -f`). Errors here mean adb itself is broken
 *     (cable unplugged, USB unauthorized, etc.). On error we keep the
 *     previous list (stale-but-visible beats empty).
 */
type State = {
  /** serial → full AppInfo list from the agent (already user-first sorted) */
  infosBySerial: Record<string, AppInfo[]>;
  /** serial → cached system info snapshot (60s TTL). */
  systemInfoBySerial: Record<string, DeviceSystemInfo>;
  /** serial → last successful system-info fetch epoch ms. */
  systemInfoFetchedAt: Record<string, number>;
  /** serial → true while a system-info fetch is in flight. */
  systemInfoLoading: Record<string, boolean>;
  /** serial → error message from last failed system-info fetch. */
  systemInfoError: Record<string, string | null>;
  /** serial → in-flight system-info fetch promise, or null. */
  systemInfoInflight: Record<string, Promise<DeviceSystemInfo | null> | null>;
  /** serial → last successful fetch epoch ms (used for TTL) */
  fetchedAt: Record<string, number>;
  /** serial → in-flight fetch promise, or null */
  inflight: Record<string, Promise<void> | null>;
  /** serial → error message from last failed fetch */
  error: Record<string, string | null>;
  /** serial → true while a fetch is in flight */
  loading: Record<string, boolean>;
  /**
   * Icon cache: serial → package → `data:image/png;base64,...` URL or
   * `null` (meaning "we tried, app has no icon resource"). Stored so
   * switching tabs doesn't refetch every icon; `drop(serial)` clears
   * it, `refresh(serial)` keeps it (icons only change on app update,
   * which also resets the package list anyway).
   */
  icons: Record<string, Record<string, string | null>>;
  /** serial → package → in-flight icon fetch promise, or null */
  iconInflight: Record<string, Record<string, Promise<string | null> | null>>;
};

type Actions = {
  /** Idempotent: triggers a fetch only if cache is stale or missing. */
  ensureLoaded: (serial: string, opts?: { ttlMs?: number }) => Promise<void>;
  /** Force-refresh: bypasses TTL, kicks a new fetch. */
  refresh: (serial: string) => Promise<void>;
  /** Drop the cache entry for one device (e.g. on disconnect). */
  drop: (serial: string) => void;
  /**
   * Read the cached icon for (serial, pkg). Returns the data URL, `null`
   * if we already know the app has no icon, or `undefined` if there's
   * no cached entry yet.
   */
  getCachedIcon: (serial: string, pkg: string) => string | null | undefined;
  /**
   * Fetch (or return cached) icon for (serial, pkg). Concurrent callers
   * coalesce; a `null` outcome is cached too so we don't repeatedly
   * hit the device for apps that have no icon resource.
   */
  fetchIcon: (
    serial: string,
    pkg: string,
    fetcher: (s: string, p: string) => Promise<string | null>,
  ) => Promise<string | null>;
  /** Idempotent: triggers a system-info fetch only if cache is stale or missing. */
  ensureSystemInfoLoaded: (serial: string, opts?: { ttlMs?: number }) => Promise<void>;
  /** Force-refresh the system info, bypassing the TTL. */
  refreshSystemInfo: (serial: string) => Promise<void>;
  /**
   * Synchronously read cached system info, or `undefined` if not yet
   * fetched (caller should `await ensureSystemInfoLoaded(serial)`).
   */
  getCachedSystemInfo: (serial: string) => DeviceSystemInfo | undefined;
};

const DEFAULT_TTL_MS = 60_000; // 1 minute — fresh enough for installs/uninstalls

async function fetchPackages(serial: string): Promise<AppInfo[]> {
  // `adbListPackagesViaAgent` already has a Rust-side fallback
  // (agent → `pm list packages -f`). Either path returns AppInfo[].
  return adbListPackagesViaAgent(serial);
}

export const usePackagesStore = create<State & Actions>((set, get) => ({
  infosBySerial: {},
  fetchedAt: {},
  inflight: {},
  error: {},
  loading: {},
  icons: {},
  iconInflight: {},
  systemInfoBySerial: {},
  systemInfoFetchedAt: {},
  systemInfoLoading: {},
  systemInfoError: {},
  systemInfoInflight: {},

  async ensureLoaded(serial, opts) {
    const ttlMs = opts?.ttlMs ?? DEFAULT_TTL_MS;
    const state = get();
    const fetchedAt = state.fetchedAt[serial] ?? 0;
    const isFresh = Date.now() - fetchedAt < ttlMs;
    const hasData = (state.infosBySerial[serial] ?? []).length > 0;
    // Skip when fresh AND we have data. Always refetch when empty so the
    // UI can show a placeholder list while loading.
    if (isFresh && hasData) return;
    if (state.inflight[serial]) {
      // Another caller is already fetching this device — wait for it.
      return state.inflight[serial]!;
    }
    const promise = (async () => {
      set((s) => ({
        loading: { ...s.loading, [serial]: true },
        error: { ...s.error, [serial]: null },
      }));
      try {
        const infos = await fetchPackages(serial);
        set((s) => ({
          infosBySerial: { ...s.infosBySerial, [serial]: infos },
          fetchedAt: { ...s.fetchedAt, [serial]: Date.now() },
          loading: { ...s.loading, [serial]: false },
          error: { ...s.error, [serial]: null },
          inflight: { ...s.inflight, [serial]: null },
        }));
      } catch (e) {
        // Keep the existing list (may be stale but better than empty).
        set((s) => ({
          error: { ...s.error, [serial]: String(e) },
          loading: { ...s.loading, [serial]: false },
          inflight: { ...s.inflight, [serial]: null },
        }));
      }
    })();
    set((s) => ({ inflight: { ...s.inflight, [serial]: promise } }));
    return promise;
  },

  async refresh(serial) {
    // Bypass TTL, kick a new fetch (coalesced with any concurrent caller).
    set((s) => ({ fetchedAt: { ...s.fetchedAt, [serial]: 0 } }));
    return get().ensureLoaded(serial, { ttlMs: 0 });
  },

  drop(serial) {
    set((s) => {
      const { [serial]: _i, ...infos } = s.infosBySerial;
      const { [serial]: _f, ...fa } = s.fetchedAt;
      const { [serial]: _l, ...ld } = s.loading;
      const { [serial]: _e, ...er } = s.error;
      const { [serial]: _ic, ...icons } = s.icons;
      const { [serial]: _ii, ...inflight } = s.iconInflight;
      const { [serial]: _si, ...sysi } = s.systemInfoBySerial;
      const { [serial]: _sf, ...sysf } = s.systemInfoFetchedAt;
      const { [serial]: _sl, ...sysl } = s.systemInfoLoading;
      const { [serial]: _se, ...syse } = s.systemInfoError;
      const { [serial]: _sif, ...sysif } = s.systemInfoInflight;
      void _i; void _f; void _l; void _e; void _ic; void _ii;
      void _si; void _sf; void _sl; void _se; void _sif;
      return {
        infosBySerial: infos,
        fetchedAt: fa,
        loading: ld,
        error: er,
        icons,
        iconInflight: inflight,
        systemInfoBySerial: sysi,
        systemInfoFetchedAt: sysf,
        systemInfoLoading: sysl,
        systemInfoError: syse,
        systemInfoInflight: sysif,
      };
    });
  },

  async ensureSystemInfoLoaded(serial, opts) {
    const ttlMs = opts?.ttlMs ?? DEFAULT_TTL_MS;
    const state = get();
    const fetchedAt = state.systemInfoFetchedAt[serial] ?? 0;
    const isFresh = Date.now() - fetchedAt < ttlMs;
    const hasData = !!state.systemInfoBySerial[serial];
    if (isFresh && hasData) return;
    const existing = state.systemInfoInflight[serial];
    if (existing) {
      // Wait for the in-flight fetch to settle; caller reads from state.
      await existing.catch(() => null);
      return;
    }
    const promise = (async () => {
      set((s) => ({
        systemInfoLoading: { ...s.systemInfoLoading, [serial]: true },
        systemInfoError: { ...s.systemInfoError, [serial]: null },
      }));
      try {
        const { adbSystemInfoViaAgent } = await import('@/ipc/adb');
        const info: DeviceSystemInfo = await adbSystemInfoViaAgent(serial);
        set((s) => ({
          systemInfoBySerial: { ...s.systemInfoBySerial, [serial]: info },
          systemInfoFetchedAt: { ...s.systemInfoFetchedAt, [serial]: Date.now() },
          systemInfoLoading: { ...s.systemInfoLoading, [serial]: false },
          systemInfoError: { ...s.systemInfoError, [serial]: null },
          systemInfoInflight: { ...s.systemInfoInflight, [serial]: null },
        }));
        return info;
      } catch (e) {
        set((s) => ({
          systemInfoError: { ...s.systemInfoError, [serial]: String(e) },
          systemInfoLoading: { ...s.systemInfoLoading, [serial]: false },
          systemInfoInflight: { ...s.systemInfoInflight, [serial]: null },
        }));
        return null;
      }
    })();
    set((s) => ({
      systemInfoInflight: { ...s.systemInfoInflight, [serial]: promise },
    }));
    // Caller reads `state.systemInfoBySerial[serial]` after await. Return void.
  },

  async refreshSystemInfo(serial) {
    set((s) => ({ systemInfoFetchedAt: { ...s.systemInfoFetchedAt, [serial]: 0 } }));
    await get().ensureSystemInfoLoaded(serial, { ttlMs: 0 });
  },

  getCachedSystemInfo(serial) {
    return get().systemInfoBySerial[serial];
  },

  /**
   * Read the cached icon for (serial, pkg). Returns:
   *   - `data:image/png;base64,...` string if cached and present
   *   - `null` if cached and we already know the app has no icon
   *   - `undefined` if the cache has no entry for this (serial, pkg) yet
   *
   * Callers distinguish `null` (no point refetching) from `undefined`
   * (need to fetch).
   */
  getCachedIcon(serial: string, pkg: string): string | null | undefined {
    const perSerial = get().icons[serial];
    if (!perSerial) return undefined;
    if (!(pkg in perSerial)) return undefined;
    return perSerial[pkg];
  },

  /**
   * Fetch (or return cached) icon for (serial, pkg). Concurrent callers
   * for the same key coalesce onto the same in-flight promise. A `null`
   * result is also cached so we don't repeatedly hit the device for apps
   * that legitimately have no icon resource.
   */
  async fetchIcon(
    serial: string,
    pkg: string,
    fetcher: (s: string, p: string) => Promise<string | null>,
  ): Promise<string | null> {
    const cached = get().getCachedIcon(serial, pkg);
    if (cached !== undefined) return cached;
    const inflight = get().iconInflight[serial] ?? {};
    if (inflight[pkg]) return inflight[pkg]!;
    const promise = (async () => {
      try {
        const dataUrl = await fetcher(serial, pkg);
        set((s) => {
          const perSerial = { ...(s.icons[serial] ?? {}), [pkg]: dataUrl };
          const { [pkg]: _done, ...rest } = s.iconInflight[serial] ?? {};
          void _done;
          return {
            icons: { ...s.icons, [serial]: perSerial },
            iconInflight: { ...s.iconInflight, [serial]: rest },
          };
        });
        return dataUrl;
      } catch (e) {
        set((s) => {
          const { [pkg]: _done, ...rest } = s.iconInflight[serial] ?? {};
          void _done;
          return { iconInflight: { ...s.iconInflight, [serial]: rest } };
        });
        throw e;
      }
    })();
    set((s) => ({
      iconInflight: {
        ...s.iconInflight,
        [serial]: { ...(s.iconInflight[serial] ?? {}), [pkg]: promise },
      },
    }));
    return promise;
  },
}));

/** Convenience selector: just the package list for `serial`. */
export function selectInfos(s: State, serial: string): AppInfo[] {
  return s.infosBySerial[serial] ?? [];
}
