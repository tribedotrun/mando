import { create } from 'zustand';
import type { ItemStatus } from '#renderer/types';

interface TaskFiltersState {
  statusFilter: ItemStatus | 'action-needed' | 'in-progress-group' | null;
  showArchived: boolean;
  setFilter: (status: TaskFiltersState['statusFilter']) => void;
  setShowArchived: (show: boolean) => void;
}

const STORAGE_KEY = 'mando:showArchived';

export const useTaskFilters = create<TaskFiltersState>((set) => ({
  statusFilter: null,
  showArchived: localStorage.getItem(STORAGE_KEY) === 'true',
  setFilter: (statusFilter) => set({ statusFilter }),
  setShowArchived: (show) => {
    localStorage.setItem(STORAGE_KEY, String(show));
    set({ showArchived: show });
  },
}));
