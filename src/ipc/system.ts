import { invoke } from '@tauri-apps/api/core';

export type AppVersionInfo = {
  /** Version baked into the Rust binary (CARGO_PKG_VERSION). */
  version: string;
  /** `"debug"` for `pnpm tauri dev`, `"release"` for production builds. */
  profile: 'debug' | 'release' | string;
  /** Version Tauri ships with, straight from `tauri.conf.json`. */
  tauriVersion: string;
};

/**
 * Fetch the runtime version + build profile. The shape is the
 * same one `commands::system::get_app_version` returns on the
 * Rust side.
 *
 * We keep this in its own module (rather than `useTauri.ts`)
 * because version info is metadata, not settings — it doesn't
 * warrant a `Settings` round-trip and it doesn't change between
 * Tauri command lifetimes.
 */
export async function getAppVersion(): Promise<AppVersionInfo> {
  return invoke<AppVersionInfo>('get_app_version');
}
