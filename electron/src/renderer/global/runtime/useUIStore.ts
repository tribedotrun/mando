import { create } from 'zustand';
import type { TaskItem } from '#renderer/global/types';

interface UIState {
  createTaskOpen: boolean;
  paletteOpen: boolean;
  shortcutsOpen: boolean;
  mergeItem: TaskItem | null;
  inlineFocusHandler: (() => void) | null;
  // Owned by AppLayout; called from anywhere via toggleSidebar().
  sidebarToggleHandler: (() => void) | null;
  openCreateTask: () => void;
  closeCreateTask: () => void;
  registerInlineFocus: (handler: () => void) => void;
  unregisterInlineFocus: () => void;
  togglePalette: () => void;
  closePalette: () => void;
  toggleShortcuts: () => void;
  closeShortcuts: () => void;
  setMergeItem: (item: TaskItem | null) => void;
  registerSidebarToggle: (handler: () => void) => void;
  unregisterSidebarToggle: () => void;
  toggleSidebar: () => void;
}

export const useUIStore = create<UIState>((set, get) => ({
  createTaskOpen: false,
  paletteOpen: false,
  shortcutsOpen: false,
  mergeItem: null,
  inlineFocusHandler: null,
  sidebarToggleHandler: null,
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
  registerSidebarToggle: (handler) => set({ sidebarToggleHandler: handler }),
  unregisterSidebarToggle: () => set({ sidebarToggleHandler: null }),
  toggleSidebar: () => {
    const handler = get().sidebarToggleHandler;
    if (handler) handler();
  },
}));
