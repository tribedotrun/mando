import { create } from 'zustand';
import type { TaskItem, ItemStatus } from '#renderer/types';
import { fetchTasks } from '#renderer/api';
import { getErrorMessage } from '#renderer/utils';
import { createFetchGenerationGuard } from '#renderer/global/stores/utils';

const SHOW_ARCHIVED_KEY = 'mando:showArchived';

const fetchGen = createFetchGenerationGuard();

interface TaskStore {
  items: TaskItem[];
  count: number;
  statusFilter: ItemStatus | 'action-needed' | 'in-progress-group' | null;
  showArchived: boolean;
  loading: boolean;
  error: string | null;
  fetch: () => Promise<void>;
  setFilter: (status: ItemStatus | 'action-needed' | 'in-progress-group' | null) => void;
  setShowArchived: (show: boolean) => void;
  optimisticUpdate: (id: number, patch: Partial<TaskItem>) => void;
}

export const useTaskStore = create<TaskStore>((set, getState) => ({
  items: [],
  count: 0,
  statusFilter: null,
  showArchived: localStorage.getItem(SHOW_ARCHIVED_KEY) === 'true',
  loading: false,
  error: null,

  fetch: async () => {
    const gen = fetchGen.next();
    set({ loading: true, error: null });
    try {
      const data = await fetchTasks(getState().showArchived);
      if (!fetchGen.isLatest(gen)) return;
      set({ items: data.items, count: data.count, loading: false });
    } catch (err) {
      if (!fetchGen.isLatest(gen)) return;
      set({ loading: false, error: getErrorMessage(err, 'Failed to fetch tasks') });
    }
  },

  optimisticUpdate: (id, patch) => {
    set((state) => ({
      items: state.items.map((item) => (item.id === id ? { ...item, ...patch } : item)),
    }));
  },

  setFilter: (status) => set({ statusFilter: status }),

  setShowArchived: (show) => {
    localStorage.setItem(SHOW_ARCHIVED_KEY, String(show));
    set({ showArchived: show });
    void getState().fetch();
  },
}));
