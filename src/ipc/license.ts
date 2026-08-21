import { invoke } from '@tauri-apps/api/core';

export type LicenseFeature = 'apk_report_export' | 'signing_v31' | 'adb_multi_device' | 'adb_batch_install';
export type LicenseState =
  | 'unlicensed'
  | 'active'
  | 'expired'
  | 'deviceMismatch'
  | 'invalid'
  | 'revoked'
  | 'serverRejected'
  /**
   * 盲 license（admin 签发时未指定 deviceId）+ 当前没配 server URL。
   * 告诉用户「这个 license 只能在线激活，去设置里配 server URL」。
   */
  | 'noDeviceBinding';
export type LicenseMode = 'offline' | 'online' | 'offline_degraded';

export type LicenseStatus = {
  state: LicenseState | string;
  deviceId: string;
  edition: 'free' | 'vip';
  licenseId: string | null;
  licensedTo: string | null;
  expiresAt: string | null;
  perpetual: boolean;
  features: string[];
  message: string | null;
  mode: LicenseMode | string;
  lastVerifiedAt: string | null;
  /**
   * 仅在线模式有意义。当 server 告知「该 license 已绑定到其他机器」时为 true。
   * UI 应在 deviceMismatch 状态下显示「替换绑定」按钮。
   * 离线 / 离线降级 / 不存在 license 时一律为 false。
   */
  canRebind: boolean;
  /** Server 当前绑定的 deviceId（仅 canRebind=true 时有值）。 */
  boundDeviceId: string | null;
};

export const getLicenseStatus = () => invoke<LicenseStatus>('get_license_status');
export const refreshLicenseStatus = () => invoke<LicenseStatus>('refresh_license_status');
export const verifyLicenseRemote = (token: string) => invoke<LicenseStatus>('verify_license_remote', { token });
export const getDeviceId = () => invoke<string>('get_device_id');
export const getLicenseServerUrl = () => invoke<string | null>('get_license_server_url');
export const activateLicense = (token: string) => invoke<LicenseStatus>('activate_license', { token });
export const removeLicense = () => invoke<LicenseStatus>('remove_license');

/**
 * 在线模式「替换绑定」：调 server 把当前 license 的 server 端绑定替换到本机。
 * 后端会读本机 license.json 里的 token，再用本机 deviceId 去 server 替换。
 * 不需要 token 参数：服务端只信任本机已激活的那一份。
 *
 * 失败场景：
 * - 本机未激活（没有 license.json） → 后端抛 "本机尚未激活 license"
 * - 未配置 server URL                → 后端抛 "未配置 license server URL"
 * - server 不可达                    → 后端抛 "调用 license-server bind 失败：..."
 */
export const replaceLicenseBinding = () => invoke<LicenseStatus>('replace_license_binding');
