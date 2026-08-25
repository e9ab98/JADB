import { invoke } from '@tauri-apps/api/core';

/**
 * Mirrors `AdbDevice` shape (serial / state / product / model) but for
 * fastboot-mode devices. `state` is always `"fastboot"` on the wire;
 * model and product are best-effort — `fastboot devices -l` only
 * reports them if the bootloader printed them at power-on, which
 * depends on the OEM and the build type.
 */
export type FastbootDevice = {
  serial: string;
  state: string;
  model: string | null;
  product: string | null;
};

/** List fastboot-mode devices. Returns an empty array if none. */
export async function fastbootDevices(): Promise<FastbootDevice[]> {
  return invoke<FastbootDevice[]>('fastboot_devices');
}

/**
 * Reboot `device` via fastboot. `mode` is `null` / `undefined` for a
 * normal reboot (back to system), `"recovery"` to drop into recovery,
 * `"bootloader"` to stay in fastboot. Errors propagate from Rust as
 * strings — callers can match the `"tool missing: fastboot"` prefix to
 * detect a missing fastboot binary.
 */
export async function fastbootReboot(
  device: string,
  mode?: 'recovery' | 'bootloader' | null,
): Promise<string> {
  return invoke<string>('fastboot_reboot', { device, mode: mode ?? null });
}

/**
 * Bootloader variables surfaced in the Fastboot tab's "Get info"
 * panel. Every field is `null` when `fastboot getvar <name>` failed
 * (variable not supported on this bootloader / device). The UI
 * renders null fields as em-dashes — missing data is not the same as
 * "loading failed", so we don't surface a per-field error toast.
 */
export type FastbootVarInfo = {
  // Identity & security
  unlocked: string | null;
  verifiedBootState: string | null;
  hardware: string | null;
  variant: string | null;
  // Versions
  versionBootloader: string | null;
  versionHardware: string | null;
  versionBaseband: string | null;
  product: string | null;
  // Flashing pre-flight
  maxDownloadSize: string | null;
  offModeCharge: string | null;
  batterySocOk: string | null;
  antiRollback: string | null;
  // Slot info
  currentSlot: string | null;
  slotCount: string | null;
  serialno: string | null;
};

/** Read the full set of UI-surfaced bootloader variables for `device`.
 *  Each `getvar` call is independent; partial failures are reported
 *  by leaving the corresponding field null in the returned struct. */
export async function fastbootGetInfo(device: string): Promise<FastbootVarInfo> {
  return invoke<FastbootVarInfo>('fastboot_get_info', { device });
}

/**
 * Raw output of `fastboot oem device-info`. Returned line-by-line
 * without any parsing so the frontend can apply OEM-specific
 * adapters (Pixel uses `(bootloader) Key: value` lines; Xiaomi uses
 * a different subset; Huawei / MTK don't support the command at all
 * and the IPC will fail with the bootloader-reported error).
 *
 * The frontend detects OEM from `FastbootDevice.product` and routes
 * to the right parser; if no parser matches, the lines are shown
 * raw in a collapsible section so power users can still read them.
 */
export type OemDeviceInfo = {
  rawLines: string[];
};

/**
 * Run `fastboot oem device-info` on `device`. Throws when the
 * bootloader doesn't support the command — the UI treats that as
 * "unsupported" and hides the vendor-diagnostics section rather
 * than showing an error toast.
 */
export async function fastbootGetOemDeviceInfo(
  device: string,
): Promise<OemDeviceInfo> {
  return invoke<OemDeviceInfo>('fastboot_oem_device_info', { device });
}
