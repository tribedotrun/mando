import { create } from 'zustand';
import type { TaskItem, ItemStatus } from '#renderer/types';
import { fetchTasks, addTask, deleteItems, type AddTaskInput } from '#renderer/api';
import { getErrorMessage } from '#renderer/utils';

const SHOW_ARCHIVED_KEY = 'mando:showArchived';

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

export const useTaskStore = create<TaskStore>((set, getState) => ({
  items: [],
  count: 0,
  statusFilter: null,
  showArchived: localStorage.getItem(SHOW_ARCHIVED_KEY) === 'true',
  loading: false,
  error: null,

  fetch: async () => {
    set({ loading: true, error: null });
    try {
      const showArchived = getState().showArchived;
      const data = await fetchTasks(showArchived);
      set({ items: data.items, count: data.count, loading: false });
    } catch (err) {
      set({
        loading: false,
        error: getErrorMessage(err, 'Failed to fetch tasks'),
      });
    }
  },

  add: async (input: AddTaskInput) => {
    try {
      await addTask(input);
      await getState().fetch();
    } catch (err) {
      set({
        error: getErrorMessage(err, 'Failed to add task'),
      });
      throw err;
    }
  },

  remove: async (ids: number[]) => {
    try {
      await deleteItems(ids);
      await getState().fetch();
    } catch (err) {
      set({
        error: getErrorMessage(err, 'Failed to delete items'),
      });
      throw err;
    }
  },

  optimisticUpdate: (id: number, patch: Partial<TaskItem>) => {
    set((state) => ({
      items: state.items.map((item) => (item.id === id ? { ...item, ...patch } : item)),
    }));
  },

  setFilter: (status: ItemStatus | 'action-needed' | 'in-progress-group' | null) =>
    set({ statusFilter: status }),

  setShowArchived: (show: boolean) => {
    localStorage.setItem(SHOW_ARCHIVED_KEY, String(show));
    set({ showArchived: show });
    // Re-fetch with updated include_archived param
    getState().fetch();
  },
}));
