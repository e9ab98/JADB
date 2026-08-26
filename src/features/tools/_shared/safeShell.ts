/**
 * `safeShell` — thin wrapper around `adbShell` that swallows the
 * "device disconnected" / "torn down mid-run" failure mode into a
 * `null` return value, so step factories can decide between "retry"
 * and "abort" without wrapping every call in a try/catch.
 *
 * Lives in `_shared/` because every step-driven card (MIUI install,
 * developer-option matrix, bug report capture, ...) needs the same
 * plumbing. Pulled out of the legacy `ToolsPanel.tsx` so the new
 * cards don't need to copy-paste the helper.
 */
import { adbShell, type ShellOutput } from '@/ipc/adb';

export async function safeShell(
  serial: string,
  command: string,
): Promise<ShellOutput | null> {
  try {
    return await adbShell(serial, command);
  } catch {
    return null;
  }
}

/** Single-quote escape for embedding an arbitrary string inside
 *  `adb shell su -c '...'`. Replaces every `'` with `'\\''`, the
 *  portable POSIX trick (close quote, escaped literal, reopen quote).
 *  Backslashes inside the input are doubled so the device's `sh`
 *  parser doesn't reinterpret them. */
export function quoteForSu(input: string): string {
  return "'" + input.replace(/\\/g, '\\\\').replace(/'/g, "'\\''") + "'";
}
