import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

const pickApkFile = vi.fn();
const pickOutDir = vi.fn();
const decompileApk = vi.fn();
const openPath = vi.fn();

vi.mock('@/ipc/decompile', () => ({
  pickApkFile: () => pickApkFile(),
  pickOutDir: () => pickOutDir(),
  decompileApk: (path: string, outDir: string, force: boolean) => decompileApk(path, outDir, force),
  openPath: (path: string) => openPath(path),
}));

// Stub sonner to avoid hitting the real DOM
vi.mock('sonner', () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

import { DecompileForm } from '@/features/decompile/DecompileForm';

describe('<DecompileForm>', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    pickApkFile.mockResolvedValue('/tmp/in.apk');
    pickOutDir.mockResolvedValue('/tmp/out');
    decompileApk.mockResolvedValue({ task_id: 'tid', kind: 'decompile' });
  });

  it('disables start until both paths chosen', () => {
    render(<DecompileForm onStarted={vi.fn()} />);
    const btn = screen.getByRole('button', { name: /Start Decompile|开始反编译/ });
    expect(btn).toBeDisabled();
  });

  it('calls decompileApk and onStarted after picks', async () => {
    const onStarted = vi.fn();
    const user = userEvent.setup();
    render(<DecompileForm onStarted={onStarted} />);

    // Click both "Pick" / "选择" buttons (translate-tolerant)
    const buttons = screen.getAllByRole('button', { name: /Pick|选择/ });
    expect(buttons.length).toBeGreaterThanOrEqual(2);
    await user.click(buttons[0]!);
    await user.click(buttons[1]!);

    const start = screen.getByRole('button', { name: /Start Decompile|开始反编译/ });
    expect(start).not.toBeDisabled();
    await user.click(start);

    expect(decompileApk).toHaveBeenCalledWith('/tmp/in.apk', '/tmp/out', false);
    expect(onStarted).toHaveBeenCalledWith({ task_id: 'tid', kind: 'decompile' });
  });
});
