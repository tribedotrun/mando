import { create } from 'zustand';
import type { TaskItem } from '#renderer/global/types';

interface UIState {
  paletteOpen: boolean;
  shortcutsOpen: boolean;
  mergeItem: TaskItem | null;
  inlineFocusHandler: (() => void) | null;
  homeNavigator: (() => void) | null;
  pendingInlineFocus: boolean;
  // Owned by AppLayout; called from anywhere via toggleSidebar().
  sidebarToggleHandler: (() => void) | null;
  openCreateTask: () => void;
  registerInlineFocus: (handler: () => void) => void;
  unregisterInlineFocus: () => void;
  registerHomeNavigator: (handler: () => void) => void;
  unregisterHomeNavigator: () => void;
  clearPendingInlineFocus: () => void;
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
  paletteOpen: false,
  shortcutsOpen: false,
  mergeItem: null,
  inlineFocusHandler: null,
  homeNavigator: null,
  pendingInlineFocus: false,
  sidebarToggleHandler: null,
  openCreateTask: () => {
    const focus = get().inlineFocusHandler;
    if (focus) {
      focus();
      return;
    }
    const navigateHome = get().homeNavigator;
    if (!navigateHome) return;
    set({ pendingInlineFocus: true });
    navigateHome();
  },
  registerInlineFocus: (handler) => set({ inlineFocusHandler: handler }),
  unregisterInlineFocus: () => set({ inlineFocusHandler: null }),
  registerHomeNavigator: (handler) => set({ homeNavigator: handler }),
  unregisterHomeNavigator: () => set({ homeNavigator: null }),
  clearPendingInlineFocus: () => set({ pendingInlineFocus: false }),
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
