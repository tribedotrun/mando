import { create } from 'zustand';
import type { TaskItem, ItemStatus } from '#renderer/types';
import { fetchTasks, addTask, deleteItems, type AddTaskInput } from '#renderer/api';
import { getErrorMessage } from '#renderer/utils';

const SHOW_ARCHIVED_KEY = 'mando:showArchived';

let fetchGeneration = 0;

interface TaskStore {
  items: TaskItem[];
  count: number;
  statusFilter: ItemStatus | 'action-needed' | 'in-progress-group' | null;
  showArchived: boolean;
  loading: boolean;
  error: string | null;
  fetch: () => Promise<void>;
  add: (input: AddTaskInput) => Promise<void>;
  remove: (ids: number[]) => Promise<void>;
  setFilter: (status: ItemStatus | 'action-needed' | 'in-progress-group' | null) => void;
  setShowArchived: (show: boolean) => void;
  optimisticUpdate: (id: number, patch: Partial<TaskItem>) => void;
}

export const useTaskStore = create<TaskStore>((set, getState) => {
  /** Run a mutation, re-fetch on success, set error + rethrow on failure. */
  async function mutate(fn: () => Promise<unknown>, errLabel: string): Promise<void> {
    try {
      await fn();
      await getState().fetch();
    } catch (err) {
      set({ error: getErrorMessage(err, errLabel) });
      throw err;
    }
  }

  return {
    items: [],
    count: 0,
    statusFilter: null,
    showArchived: localStorage.getItem(SHOW_ARCHIVED_KEY) === 'true',
    loading: false,
    error: null,

    fetch: async () => {
      const gen = ++fetchGeneration;
      set({ loading: true, error: null });
      try {
        const data = await fetchTasks(getState().showArchived);
        if (gen !== fetchGeneration) return; // stale response
        set({ items: data.items, count: data.count, loading: false });
      } catch (err) {
        if (gen !== fetchGeneration) return;
        set({ loading: false, error: getErrorMessage(err, 'Failed to fetch tasks') });
      }
    },

    add: (input) => mutate(() => addTask(input), 'Failed to add task'),
    remove: (ids) => mutate(() => deleteItems(ids), 'Failed to delete items'),

    optimisticUpdate: (id, patch) => {
      set((state) => ({
        items: state.items.map((item) => (item.id === id ? { ...item, ...patch } : item)),
      }));
    },

    setFilter: (status) => set({ statusFilter: status }),

    setShowArchived: (show) => {
      localStorage.setItem(SHOW_ARCHIVED_KEY, String(show));
      set({ showArchived: show });
      getState().fetch();
    },
  };
});
