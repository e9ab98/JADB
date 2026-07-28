import { create } from 'zustand';
import {
  getToolStatus,
  installTool,
  removeTool,
  onInstallProgress,
  type ToolName,
  type ToolStatus,
  type InstallProgress,
} from '@/ipc/tools';

type ToolsState = {
  tools: ToolStatus[];
  busy: ToolName | null;
  progress: Partial<Record<ToolName, InstallProgress>>;
  error: string | null;
  refresh: () => Promise<void>;
  install: (name: ToolName) => Promise<void>;
  remove: (name: ToolName) => Promise<void>;
};

export const useToolsStore = create<ToolsState>((set, get) => ({
  tools: [],
  busy: null,
  progress: {},
  error: null,
  async refresh() {
    try {
      const tools = await getToolStatus();
      set({ tools, error: null });
    } catch (e) {
      set({ error: String(e) });
    }
  },
  async install(name) {
    if (get().busy) return;
    set({ busy: name, error: null });
    try {
      await installTool(name);
      await get().refresh();
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ busy: null, progress: { ...get().progress, [name]: undefined } });
    }
  },
  async remove(name) {
    if (get().busy) return;
    set({ busy: name, error: null });
    try {
      await removeTool(name);
      await get().refresh();
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ busy: null });
    }
  },
}));

// Subscribe to install-progress events globally.
onInstallProgress((p) => {
  useToolsStore.setState((s) => ({
    progress: { ...s.progress, [p.name]: p },
  }));
}).catch((e) => {
  // eslint-disable-next-line no-console
  console.error('failed to subscribe to tool://install-progress', e);
});
