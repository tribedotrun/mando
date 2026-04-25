import { create } from 'zustand';

/**
 * Ephemeral per-transcript UI state. Separate from the React Query event
 * cache so scroll-lock / expansion toggles don't force refetches.
 *
 * `toolOpenState` is a Map rather than a Set so `undefined` means "user has
 * not interacted; fall back to the call site's `defaultOpen`", while an
 * explicit `false` records a user-driven collapse and overrides defaults.
 */
interface TranscriptUiState {
  toolOpenState: Map<string, boolean>;
  thinkingOpenIds: Set<string>;
  stickToBottom: boolean;
  searchQuery: string;
  setToolExpanded: (id: string, open: boolean) => void;
  toggleThinking: (id: string) => void;
  setStickToBottom: (stick: boolean) => void;
  setSearchQuery: (query: string) => void;
  resetForSession: () => void;
}

export const useTranscriptUi = create<TranscriptUiState>((set) => ({
  toolOpenState: new Map<string, boolean>(),
  thinkingOpenIds: new Set<string>(),
  stickToBottom: true,
  searchQuery: '',
  setToolExpanded: (id, open) =>
    set((prev) => {
      const next = new Map(prev.toolOpenState);
      next.set(id, open);
      return { toolOpenState: next };
    }),
  toggleThinking: (id) =>
    set((prev) => {
      const next = new Set(prev.thinkingOpenIds);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return { thinkingOpenIds: next };
    }),
  setStickToBottom: (stick) => set({ stickToBottom: stick }),
  setSearchQuery: (query) => set({ searchQuery: query }),
  resetForSession: () =>
    set({
      toolOpenState: new Map<string, boolean>(),
      thinkingOpenIds: new Set<string>(),
      stickToBottom: true,
      searchQuery: '',
    }),
}));

export const selectToolOpenState = (id: string) => (state: TranscriptUiState) =>
  state.toolOpenState.get(id);

export const selectIsThinkingOpen = (id: string) => (state: TranscriptUiState) =>
  state.thinkingOpenIds.has(id);
