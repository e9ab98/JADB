import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

const repackageApk = vi.fn();
const pickSrcDir = vi.fn();
const pickOutApk = vi.fn();
const listSignatures = vi.fn();

vi.mock('@/ipc/repackage', () => ({
  repackageApk: (srcDir: string, outApk: string, sign: boolean, signatureId: string | null) =>
    repackageApk(srcDir, outApk, sign, signatureId),
  pickSrcDir: () => pickSrcDir(),
  pickOutApk: () => pickOutApk(),
}));

vi.mock('@/ipc/signatures', () => ({
  listSignatures: () => listSignatures(),
}));

vi.mock('sonner', () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

import { RepackageForm } from '@/features/repackage/RepackageForm';

describe('<RepackageForm>', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    pickSrcDir.mockResolvedValue('/tmp/src');
    pickOutApk.mockResolvedValue('/tmp/out.apk');
    repackageApk.mockResolvedValue({ task_id: 'tid', kind: 'repackage' });
    listSignatures.mockResolvedValue([]);
  });

  it('disables start until both paths chosen', () => {
    render(<RepackageForm onStarted={vi.fn()} />);
    const btn = screen.getByRole('button', { name: /Start Repackage|开始重打包/ });
    expect(btn).toBeDisabled();
  });

  it('calls repackageApk and onStarted after picks', async () => {
    const onStarted = vi.fn();
    const user = userEvent.setup();
    render(<RepackageForm onStarted={onStarted} />);

    const buttons = screen.getAllByRole('button', { name: /Pick|选择|Save|保存/ });
    expect(buttons.length).toBeGreaterThanOrEqual(2);
    await user.click(buttons[0]!);
    await user.click(buttons[1]!);

    const start = screen.getByRole('button', { name: /Start Repackage|开始重打包/ });
    expect(start).not.toBeDisabled();
    await user.click(start);

    expect(repackageApk).toHaveBeenCalledWith('/tmp/src', '/tmp/out.apk', false, null);
    expect(onStarted).toHaveBeenCalledWith({ task_id: 'tid', kind: 'repackage' });
  });
});
