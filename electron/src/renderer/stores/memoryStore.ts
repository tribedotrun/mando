import { create } from 'zustand';
import type { DecisionEntry, Pattern, JournalTotals } from '#renderer/types';
import { fetchJournal, fetchPatterns, updatePatternStatus, runDistiller } from '#renderer/api';
import { getErrorMessage } from '#renderer/utils';

interface MemoryStore {
  // Journal state
  decisions: DecisionEntry[];
  totals: JournalTotals;
  journalLoading: boolean;
  journalError: string | null;
  workerFilter: string;
  actionFilter: string;
  outcomeFilter: string;
  journalLimit: number;

  // Patterns state
  patterns: Pattern[];
  patternsLoading: boolean;
  patternsError: string | null;
  patternStatusFilter: string;

  // Distiller state
  distillerRunning: boolean;
  distillerResult: string | null;

  // Actions
  fetchJournal: () => Promise<void>;
  fetchPatterns: () => Promise<void>;
  setWorkerFilter: (w: string) => void;
  setActionFilter: (a: string) => void;
  setOutcomeFilter: (o: string) => void;
  setPatternStatusFilter: (s: string) => void;
  approvePattern: (id: number) => Promise<void>;
  dismissPattern: (id: number) => Promise<void>;
  runDistiller: () => Promise<void>;
}

export const useMemoryStore = create<MemoryStore>((set, getState) => ({
  decisions: [],
  totals: { total: 0, successes: 0, failures: 0, unresolved: 0 },
  journalLoading: false,
  journalError: null,
  workerFilter: '',
  actionFilter: '',
  outcomeFilter: '',
  journalLimit: 50,

  patterns: [],
  patternsLoading: false,
  patternsError: null,
  patternStatusFilter: '',

  distillerRunning: false,
  distillerResult: null,

  fetchJournal: async () => {
    const { workerFilter, journalLimit } = getState();
    set({ journalLoading: true, journalError: null });
    try {
      const data = await fetchJournal({
        worker: workerFilter || undefined,
        limit: journalLimit,
      });
      set({ decisions: data.decisions, totals: data.totals, journalLoading: false });
    } catch (err) {
      set({
        journalLoading: false,
        journalError: getErrorMessage(err, 'Failed to fetch journal'),
      });
    }
  },

  fetchPatterns: async () => {
    const { patternStatusFilter } = getState();
    set({ patternsLoading: true, patternsError: null });
    try {
      const data = await fetchPatterns(patternStatusFilter || undefined);
      set({ patterns: data.patterns, patternsLoading: false });
    } catch (err) {
      set({
        patternsLoading: false,
        patternsError: getErrorMessage(err, 'Failed to fetch patterns'),
      });
    }
  },

  setWorkerFilter: (w) => {
    set({ workerFilter: w });
    getState().fetchJournal();
  },
  setActionFilter: (a) => set({ actionFilter: a }),
  setOutcomeFilter: (o) => set({ outcomeFilter: o }),
  setPatternStatusFilter: (s) => {
    set({ patternStatusFilter: s });
    getState().fetchPatterns();
  },

  approvePattern: async (id) => {
    try {
      await updatePatternStatus(id, 'approved');
      await getState().fetchPatterns();
    } catch (err) {
      set({ patternsError: getErrorMessage(err, 'Failed to approve') });
    }
  },

  dismissPattern: async (id) => {
    try {
      await updatePatternStatus(id, 'dismissed');
      await getState().fetchPatterns();
    } catch (err) {
      set({ patternsError: getErrorMessage(err, 'Failed to dismiss') });
    }
  },

  runDistiller: async () => {
    set({ distillerRunning: true, distillerResult: null });
    try {
      const result = await runDistiller();
      set({ distillerRunning: false, distillerResult: result.summary });
      await getState().fetchPatterns();
      await getState().fetchJournal();
    } catch (err) {
      set({
        distillerRunning: false,
        distillerResult: getErrorMessage(err, 'Distiller failed'),
      });
    }
  },
}));
