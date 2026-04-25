import React, { useState, useCallback, useRef } from 'react';
import { useSessionsList } from '#renderer/domains/sessions/repo/queries';
import { useViewKeyHandler } from '#renderer/global/runtime/useKeyboardShortcuts';
import type { SessionCategory, SessionEntry } from '#renderer/global/types';
import { getErrorMessage, indexNext, indexPrev } from '#renderer/global/service/utils';
import { copyToClipboard } from '#renderer/global/runtime/useFeedback';
import {
  buildSessionSequence,
  sortCategories,
  buildResumeCmd,
  type SessionStatusFilter,
} from '#renderer/domains/sessions/service/helpers';

interface UseSessionsCardOptions {
  active: boolean;
  onOpenSession?: (s: SessionEntry) => void;
}

export interface SessionsCardState {
  pagination: {
    page: number;
    setPage: React.Dispatch<React.SetStateAction<number>>;
    totalPages: number;
  };
  filters: {
    category: SessionCategory | '';
    status: SessionStatusFilter;
    setStatus: (v: SessionStatusFilter) => void;
    menuOpen: boolean;
    setMenuOpen: (v: boolean) => void;
    isFiltered: boolean;
    handleCatClick: (cat: SessionCategory | '') => void;
  };
  list: {
    sessions: SessionEntry[];
    loading: boolean;
    error: string | null;
    hasLoadedRef: React.MutableRefObject<boolean>;
    sessionSeqMap: ReturnType<typeof buildSessionSequence>;
    clampedFocusedIndex: number;
  };
  categories: {
    counts: Record<string, number>;
    total: number;
    sorted: SessionCategory[];
  };
  actions: {
    openSession: (s: SessionEntry) => void;
  };
}

export function useSessionsCard({
  active,
  onOpenSession,
}: UseSessionsCardOptions): SessionsCardState {
  const [page, setPage] = useState(1);
  const [filterCategory, setFilterCategory] = useState<SessionCategory | ''>('');
  const [filterStatus, setFilterStatus] = useState<SessionStatusFilter>('all');
  const [focusedIndex, setFocusedIndex] = useState(-1);

  const {
    data: sessionsData,
    isLoading: loading,
    error: sessionsError,
  } = useSessionsList(
    page,
    filterCategory || undefined,
    filterStatus === 'all' ? undefined : filterStatus,
  );
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

  const handleCatClick = (cat: SessionCategory | '') => {
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
    pagination: { page, setPage, totalPages },
    filters: {
      category: filterCategory,
      status: filterStatus,
      setStatus: setFilterStatus,
      menuOpen: showFilterMenu,
      setMenuOpen: setShowFilterMenu,
      isFiltered,
      handleCatClick,
    },
    list: {
      sessions,
      loading,
      error,
      hasLoadedRef,
      sessionSeqMap,
      clampedFocusedIndex,
    },
    categories: { counts: categories, total: allTotal, sorted: sortedCats },
    actions: { openSession },
  };
}
