import { create } from 'zustand';
import {
  listTerminals,
  createTerminal,
  deleteTerminal,
  type TerminalSessionInfo,
} from '#renderer/api-terminal';
import log from '#renderer/logger';

interface TerminalState {
  sessions: TerminalSessionInfo[];
  loading: boolean;

  fetch: () => Promise<void>;
  addSession: (params: {
    project: string;
    cwd: string;
    agent: 'claude' | 'codex';
    resume_session_id?: string;
    size?: { rows: number; cols: number };
  }) => Promise<TerminalSessionInfo>;
  removeSession: (id: string) => Promise<void>;
  updateSession: (id: string, updates: Partial<TerminalSessionInfo>) => void;
}

export const useTerminalStore = create<TerminalState>((set) => ({
  sessions: [],
  loading: false,

  fetch: async () => {
    set({ loading: true });
    try {
      const sessions = await listTerminals();
      set({ sessions, loading: false });
    } catch (e) {
      log.error('Failed to fetch terminal sessions', e);
      set({ loading: false });
    }
  },

  addSession: async (params) => {
    const session = await createTerminal(params);
    set((s) => ({ sessions: [...s.sessions, session] }));
    return session;
  },

  removeSession: async (id) => {
    await deleteTerminal(id);
    set((s) => ({ sessions: s.sessions.filter((sess) => sess.id !== id) }));
  },

  updateSession: (id, updates) => {
    set((s) => ({
      sessions: s.sessions.map((sess) => (sess.id === id ? { ...sess, ...updates } : sess)),
    }));
  },
}));
