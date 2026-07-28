import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

const signApk = vi.fn();
const checkApkSigned = vi.fn();
const pickApk = vi.fn();
const listSignatures = vi.fn();

vi.mock('@/ipc/sign', () => ({
  checkApkSigned: (apkPath: string) => checkApkSigned(apkPath),
  signApk: (request: unknown) => signApk(request),
  pickApk: () => pickApk(),
}));

vi.mock('@/ipc/signatures', () => ({
  listSignatures: () => listSignatures(),
}));

const listLineages = vi.fn();
vi.mock('@/ipc/lineages', () => ({
  listLineages: () => listLineages(),
}));

vi.mock('sonner', () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

import { SignForm } from '@/features/sign/SignForm';

describe('<SignForm>', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    checkApkSigned.mockResolvedValue(false);
    signApk.mockResolvedValue({ task_id: 'tid', kind: 'sign' });
    pickApk.mockResolvedValue('/tmp/in.apk');
    listLineages.mockResolvedValue([]);
    listSignatures.mockResolvedValue([
      {
        id: 's1',
        label: 'My Key',
        keystorePath: '/ks',
        keystorePassword: 'p',
        keyAlias: 'a',
        keyPassword: 'p',
        createdAt: '2026-01-01',
      },
    ]);
  });

  it('disables start until apk + signature chosen', async () => {
    render(<SignForm onStarted={vi.fn()} />);
    const startBtn = screen.getByRole('button', { name: /Sign|签名/ });
    expect(startBtn).toBeDisabled();

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /Pick|选择/ }));
    // wait for listSignatures to resolve
    await screen.findByText('My Key');
    await user.selectOptions(screen.getByRole('combobox'), 's1');

    expect(startBtn).not.toBeDisabled();
  });
});
