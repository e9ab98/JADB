import { act, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { AdbAppsTab } from '@/features/adb/AdbAppsTab';
import {
  adbAppIcon,
  adbAppInfo,
  adbListPackages,
  adbUninstall,
} from '@/ipc/adb';

vi.mock('@/ipc/adb', () => ({
  adbAppIcon: vi.fn(),
  adbAppInfo: vi.fn(),
  adbListPackages: vi.fn(),
  adbUninstall: vi.fn(),
}));

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe('<AdbAppsTab>', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(adbListPackages).mockResolvedValue(['com.example.app']);
    vi.mocked(adbAppInfo).mockResolvedValue({
      packageName: 'com.example.app',
      appLabel: 'Example App',
      versionName: '1.0.0',
      versionCode: '1',
      minSdk: '24',
      targetSdk: '35',
      apkPath: '/data/app/example/base.apk',
      iconPath: 'res/mipmap-anydpi-v26/ic_launcher.xml',
      iconDataUrl: null,
      isSystem: false,
      isDebuggable: false,
    });
    vi.mocked(adbUninstall).mockResolvedValue('Success');
  });

  it('clears icon loading after a request resolves during a state update', async () => {
    const icon = deferred<string | null>();
    vi.mocked(adbAppIcon).mockReturnValue(icon.promise);

    render(<AdbAppsTab serial="device-1" />);

    await waitFor(() => expect(adbAppIcon).toHaveBeenCalledWith('device-1', 'com.example.app'), {
      timeout: 3000,
    });

    icon.resolve('data:image/png;base64,ZmFrZQ==');

    await waitFor(() => {
      expect(screen.getByRole('img', { name: 'Example App' })).toHaveAttribute(
        'src',
        'data:image/png;base64,ZmFrZQ==',
      );
    });
  });

  it('does not retry an icon after the backend reports no icon', async () => {
    vi.mocked(adbAppIcon).mockResolvedValue(null);

    render(<AdbAppsTab serial="device-1" />);

    await waitFor(() => expect(adbAppIcon).toHaveBeenCalledTimes(1), {
      timeout: 3000,
    });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 1200));
    });

    expect(adbAppIcon).toHaveBeenCalledTimes(1);
  });

  it('shows the debug build badge for a debuggable package', async () => {
    vi.mocked(adbAppInfo).mockResolvedValue({
      packageName: 'com.example.app',
      appLabel: 'Example App',
      versionName: '1.0.0',
      versionCode: '1',
      minSdk: '24',
      targetSdk: '35',
      apkPath: '/data/app/example/base.apk',
      iconPath: null,
      iconDataUrl: null,
      isSystem: false,
      isDebuggable: true,
    });
    vi.mocked(adbAppIcon).mockResolvedValue(null);

    render(<AdbAppsTab serial="device-1" />);

    await waitFor(() => expect(screen.getByText('Debug')).toBeInTheDocument());
  });
});
