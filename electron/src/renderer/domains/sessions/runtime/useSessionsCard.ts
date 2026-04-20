import React, { useState, useCallback, useRef } from 'react';
import { useSessionsList } from '#renderer/domains/sessions/repo/queries';
import { useViewKeyHandler } from '#renderer/global/runtime/useKeyboardShortcuts';
import type { SessionEntry } from '#renderer/global/types';
import { getErrorMessage, indexNext, indexPrev } from '#renderer/global/service/utils';
import { copyToClipboard } from '#renderer/global/runtime/useFeedback';
import {
  buildSessionSequence,
  sortCategories,
  buildResumeCmd,
} from '#renderer/domains/sessions/service/helpers';

interface UseSessionsCardOptions {
  active: boolean;
  onOpenSession?: (s: SessionEntry) => void;
}

export interface SessionsCardState {
  page: number;
  setPage: React.Dispatch<React.SetStateAction<number>>;
  filterCategory: string;
  filterStatus: string;
  setFilterStatus: (v: string) => void;
  showFilterMenu: boolean;
  setShowFilterMenu: (v: boolean) => void;
  sessions: SessionEntry[];
  totalPages: number;
  categories: Record<string, number>;
  loading: boolean;
  error: string | null;
  hasLoadedRef: React.MutableRefObject<boolean>;
  sessionSeqMap: ReturnType<typeof buildSessionSequence>;
  clampedFocusedIndex: number;
  isFiltered: boolean;
  allTotal: number;
  sortedCats: string[];
  handleCatClick: (cat: string) => void;
  openSession: (s: SessionEntry) => void;
}

export function useSessionsCard({
  active,
  onOpenSession,
}: UseSessionsCardOptions): SessionsCardState {
  const [page, setPage] = useState(1);
  const [filterCategory, setFilterCategory] = useState('');
  const [filterStatus, setFilterStatus] = useState<string>('all');
  const [focusedIndex, setFocusedIndex] = useState(-1);

  const {
    data: sessionsData,
    isLoading: loading,
    error: sessionsError,
  } = useSessionsList(page, filterCategory || undefined, filterStatus);
  // Track whether we've ever loaded data -- suppress loading text on category switches
  const hasLoadedRef = useRef(false);
  if (sessionsData) hasLoadedRef.current = true;

  const sessions: SessionEntry[] = sessionsData?.sessions ?? [];
  const totalPages = sessionsData?.total_pages ?? 1;
  const categories: Record<string, number> = sessionsData?.categories ?? {};
  const error = sessionsError ? getErrorMessage(sessionsError, 'Failed to fetch sessions') : null;
  const sessionSeqMap = React.useMemo(() => buildSessionSequence(sessions), [sessions]);

  const [showFilterMenu, setShowFilterMenu] = useState(false);
  const isFiltered = filterStatus !== 'all';

  // Clamp focusedIndex inline -- derived from sessions.length
  const clampedFocusedIndex =
    sessions.length === 0
      ? -1
      : focusedIndex >= sessions.length
        ? sessions.length - 1
        : focusedIndex;

  const openSession = useCallback(
    (s: SessionEntry) => {
      onOpenSession?.(s);
    },
    [onOpenSession],
  );

  const handleCatClick = (cat: string) => {
    setFilterCategory(cat);
    setPage(1);
  };

  const resumeCmd = (s: SessionEntry) => buildResumeCmd(s.session_id, s.resume_cwd || s.cwd);

  const handleKey = useCallback(
    (key: string, e: KeyboardEvent) => {
      switch (key) {
        case 'j':
          e.preventDefault();
          setFocusedIndex((i) => indexNext(i, sessions.length - 1));
          break;
        case 'k':
          e.preventDefault();
          setFocusedIndex((i) => indexPrev(i));
          break;
        case 'Enter': {
          const s = sessions[clampedFocusedIndex];
          if (s) {
            e.preventDefault();
            openSession(s);
          }
          break;
        }
        case 'c': {
          const s = sessions[clampedFocusedIndex];
          if (s) {
            e.preventDefault();
            void copyToClipboard(resumeCmd(s), 'Command copied');
          }
          break;
        }
        case 'Escape':
          if (clampedFocusedIndex >= 0) {
            e.preventDefault();
            setFocusedIndex(-1);
          }
          break;
      }
    },
    [sessions, clampedFocusedIndex, openSession],
  );

  useViewKeyHandler(handleKey, active);

  const allTotal = Object.values(categories).reduce((a, b) => a + b, 0);
  const sortedCats = sortCategories(categories);

  return {
    page,
    setPage,
    filterCategory,
    filterStatus,
    setFilterStatus,
    showFilterMenu,
    setShowFilterMenu,
    sessions,
    totalPages,
    categories,
    loading,
    error,
    hasLoadedRef,
    sessionSeqMap,
    clampedFocusedIndex,
    isFiltered,
    allTotal,
    sortedCats,
    handleCatClick,
    openSession,
  };
}
