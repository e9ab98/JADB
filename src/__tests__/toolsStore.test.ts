import { describe, it, expect, vi, beforeEach } from 'vitest';

const installMock = vi.fn(async (name: string) => ({
  name,
  installed: true as const,
  version: '1.0.0',
  path: '/tmp/' + name,
  downloadUrl: 'https://example.com/' + name,
}));

const removeMock = vi.fn(async (_name: string) => undefined as void);

const getStatusMock = vi.fn(async () => [
  {
    name: 'apktool' as const,
    installed: false,
    version: '2.12.1',
    path: null,
    downloadUrl: 'https://example.com/apktool.jar',
  },
  {
    name: 'jadx' as const,
    installed: false,
    version: '1.5.6',
    path: null,
    downloadUrl: 'https://example.com/jadx.zip',
  },
]);

const onProgressMock = vi.fn(async (_cb: (_p: unknown) => void) => () => undefined);

vi.mock('@/ipc/tools', () => ({
  getToolStatus: () => getStatusMock(),
  installTool: (name: string) => installMock(name),
  removeTool: (name: string) => removeMock(name),
  onInstallProgress: (cb: (_p: unknown) => void) => onProgressMock(cb),
}));

describe('useToolsStore', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.resetModules();
  });

  it('refresh populates tools', async () => {
    const { useToolsStore } = await import('@/store/tools');
    await useToolsStore.getState().refresh();
    expect(useToolsStore.getState().tools).toHaveLength(2);
  });

  it('install dispatches installTool and refreshes', async () => {
    const { useToolsStore } = await import('@/store/tools');
    await useToolsStore.getState().refresh();
    vi.clearAllMocks();
    await useToolsStore.getState().install('apktool');
    expect(installMock).toHaveBeenCalledWith('apktool');
    expect(getStatusMock).toHaveBeenCalled();
  });

  it('remove dispatches removeTool', async () => {
    const { useToolsStore } = await import('@/store/tools');
    await useToolsStore.getState().refresh();
    vi.clearAllMocks();
    await useToolsStore.getState().remove('apktool');
    expect(removeMock).toHaveBeenCalledWith('apktool');
  });

  it('subscribes to install progress on import', async () => {
    vi.resetModules();
    await import('@/store/tools');
    expect(onProgressMock).toHaveBeenCalled();
  });
});
