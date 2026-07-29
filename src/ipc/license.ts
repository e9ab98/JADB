import { invoke } from '@tauri-apps/api/core';

export type LicenseFeature = 'apk_report_export' | 'signing_v31' | 'adb_multi_device';
export type LicenseState = 'unlicensed' | 'active' | 'expired' | 'deviceMismatch' | 'invalid';

export type LicenseStatus = {
  state: LicenseState;
  deviceId: string;
  edition: 'free' | 'vip';
  licenseId: string | null;
  licensedTo: string | null;
  expiresAt: string | null;
  perpetual: boolean;
  features: string[];
  message: string | null;
};

export const getLicenseStatus = () => invoke<LicenseStatus>('get_license_status');
export const getDeviceId = () => invoke<string>('get_device_id');
export const activateLicense = (token: string) => invoke<LicenseStatus>('activate_license', { token });
export const removeLicense = () => invoke<LicenseStatus>('remove_license');
