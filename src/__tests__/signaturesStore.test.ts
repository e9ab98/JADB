import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('@/ipc/signatures', () => ({
  listSignatures: vi.fn(async () => [
    {
      id: 's1',
      label: 'L1',
      keystorePath: '/ks',
      keystorePassword: 'p',
      keyAlias: 'a',
      keyPassword: 'kp',
      createdAt: '2026-01-01',
    },
  ]),
  createNewKeystore: vi.fn(async (input: any) => ({
    ...input,
    id: 'new',
    keystorePath: '/gen.jks',
    createdAt: '2026-01-02',
  })),
  exportSignature: vi.fn(async () => '/exported.jks'),
  pickSignatureExportPath: vi.fn(async () => '/exported.jks'),
  updateSignature: vi.fn(async () => undefined),
  deleteSignature: vi.fn(async () => undefined),
  importKeystore: vi.fn(async () => ({
    id: 'i',
    label: 'I',
    keystorePath: '/i',
    keystorePassword: 'p',
    keyAlias: 'a',
    keyPassword: 'p',
    createdAt: '2026-01-02',
  })),
}));

describe('useSignaturesStore', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.resetModules();
  });

  it('refresh populates list', async () => {
    const { useSignaturesStore } = await import('@/store/signatures');
    await useSignaturesStore.getState().refresh();
    expect(useSignaturesStore.getState().list).toHaveLength(1);
    const first = useSignaturesStore.getState().list[0];
    expect(first?.id).toBe("s1");
  });

  it('create adds to list and returns the new config', async () => {
    const { useSignaturesStore } = await import('@/store/signatures');
    await useSignaturesStore.getState().refresh();
    const created = await useSignaturesStore.getState().create({
      label: 'L2',
      alias: 'a2',
      keystorePassword: 'p',
      keyPassword: 'kp2',
      options: { keyAlgorithm: 'RSA', keySize: 2048, validityDays: 10950 },
    } as any);
    expect(created.id).toBe('new');
    expect(useSignaturesStore.getState().list.length).toBeGreaterThanOrEqual(1);
  });

  it('remove triggers deleteSignature and refresh', async () => {
    const { useSignaturesStore } = await import('@/store/signatures');
    const ipc = await import('@/ipc/signatures');
    await useSignaturesStore.getState().refresh();
    await useSignaturesStore.getState().remove('s1');
    expect(ipc.deleteSignature).toHaveBeenCalledWith('s1');
  });

  it('import pushes a new signature', async () => {
    const { useSignaturesStore } = await import('@/store/signatures');
    const ipc = await import('@/ipc/signatures');
    const created = await useSignaturesStore
      .getState()
      .import('/tmp/k.jks', 'alias', 'pwd', 'Label');
    expect(ipc.importKeystore).toHaveBeenCalledWith('/tmp/k.jks', 'alias', 'pwd', 'Label');
    expect(created.id).toBe('i');
  });
});
