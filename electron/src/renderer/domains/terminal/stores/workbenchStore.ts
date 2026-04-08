import { create } from 'zustand';
import {
  fetchWorkbenches as apiFetchWorkbenches,
  archiveWorkbench as apiArchiveWorkbench,
  type WorkbenchItem,
} from '#renderer/api-terminal';
import log from '#renderer/logger';

interface WorkbenchState {
  workbenches: WorkbenchItem[];
  loading: boolean;

  fetch: () => Promise<void>;
  archive: (id: number) => Promise<void>;
}

export const useWorkbenchStore = create<WorkbenchState>((set) => ({
  workbenches: [],
  loading: false,

  fetch: async () => {
    set({ loading: true });
    try {
      const workbenches = await apiFetchWorkbenches();
      set({ workbenches, loading: false });
    } catch (e) {
      log.error('Failed to fetch workbenches', e);
      set({ loading: false });
    }
  },

  archive: async (id) => {
    await apiArchiveWorkbench(id);
    set((s) => ({
      workbenches: s.workbenches.filter((wb) => wb.id !== id),
    }));
  },
}));
