import { create } from 'zustand';
import type { TaskItem } from '#renderer/types';

interface UIState {
  createTaskOpen: boolean;
  paletteOpen: boolean;
  shortcutsOpen: boolean;
  mergeItem: TaskItem | null;
  openCreateTask: () => void;
  closeCreateTask: () => void;
  togglePalette: () => void;
  closePalette: () => void;
  toggleShortcuts: () => void;
  closeShortcuts: () => void;
  setMergeItem: (item: TaskItem | null) => void;
}

export const useUIStore = create<UIState>((set) => ({
  createTaskOpen: false,
  paletteOpen: false,
  shortcutsOpen: false,
  mergeItem: null,
  openCreateTask: () => set({ createTaskOpen: true }),
  closeCreateTask: () => set({ createTaskOpen: false }),
  togglePalette: () => set((s) => ({ paletteOpen: !s.paletteOpen })),
  closePalette: () => set({ paletteOpen: false }),
  toggleShortcuts: () => set((s) => ({ shortcutsOpen: !s.shortcutsOpen })),
  closeShortcuts: () => set({ shortcutsOpen: false }),
  setMergeItem: (item) => set({ mergeItem: item }),
}));
