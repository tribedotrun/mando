import { create } from 'zustand';

/**
 * Ephemeral per-transcript UI state. Separate from the React Query event
 * cache so scroll-lock / expansion toggles don't force refetches.
 *
 * Tool rows use explicit overrides because their defaults vary by tool type.
 */
interface TranscriptUiState {
  toolOpenState: Map<string, boolean>;
  stickToBottom: boolean;
  searchQuery: string;
  setToolExpanded: (id: string, open: boolean) => void;
  setStickToBottom: (stick: boolean) => void;
  setSearchQuery: (query: string) => void;
  resetForSession: () => void;
}

export const useTranscriptUi = create<TranscriptUiState>((set) => ({
  toolOpenState: new Map<string, boolean>(),
  stickToBottom: true,
  searchQuery: '',
  setToolExpanded: (id, open) =>
    set((prev) => {
      const next = new Map(prev.toolOpenState);
      next.set(id, open);
      return { toolOpenState: next };
    }),
  setStickToBottom: (stick) => set({ stickToBottom: stick }),
  setSearchQuery: (query) => set({ searchQuery: query }),
  resetForSession: () =>
    set({
      toolOpenState: new Map<string, boolean>(),
      stickToBottom: true,
      searchQuery: '',
    }),
}));

export const selectToolOpenState = (id: string) => (state: TranscriptUiState) =>
  state.toolOpenState.get(id);
