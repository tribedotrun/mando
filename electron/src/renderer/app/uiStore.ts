import { create } from 'zustand';
import type { TaskItem } from '#renderer/types';

interface UIState {
  createTaskOpen: boolean;
  paletteOpen: boolean;
  shortcutsOpen: boolean;
  mergeItem: TaskItem | null;
  inlineFocusHandler: (() => void) | null;
  openCreateTask: () => void;
  closeCreateTask: () => void;
  registerInlineFocus: (handler: () => void) => void;
  unregisterInlineFocus: () => void;
  togglePalette: () => void;
  closePalette: () => void;
  toggleShortcuts: () => void;
  closeShortcuts: () => void;
  setMergeItem: (item: TaskItem | null) => void;
}

export const useUIStore = create<UIState>((set, get) => ({
  createTaskOpen: false,
  paletteOpen: false,
  shortcutsOpen: false,
  mergeItem: null,
  inlineFocusHandler: null,
  openCreateTask: () => {
    const handler = get().inlineFocusHandler;
    if (handler) {
      handler();
    } else {
      set({ createTaskOpen: true });
    }
  },
  closeCreateTask: () => set({ createTaskOpen: false }),
  registerInlineFocus: (handler) => set({ inlineFocusHandler: handler }),
  unregisterInlineFocus: () => set({ inlineFocusHandler: null }),
  togglePalette: () => set((s) => ({ paletteOpen: !s.paletteOpen })),
  closePalette: () => set({ paletteOpen: false }),
  toggleShortcuts: () => set((s) => ({ shortcutsOpen: !s.shortcutsOpen })),
  closeShortcuts: () => set({ shortcutsOpen: false }),
  setMergeItem: (item) => set({ mergeItem: item }),
}));
