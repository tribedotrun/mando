import { useCallback, useRef, useState, type Dispatch, type SetStateAction } from 'react';
import {
  useScoutList,
  useScoutRefresh,
  useScoutBulkUpdate,
  useScoutBulkDelete,
  type ScoutQueryParams,
} from '#renderer/domains/scout/runtime/hooks';
import { useViewKeyHandler } from '#renderer/global/runtime/useKeyboardShortcuts';
import { useSelection } from '#renderer/global/runtime/useSelection';
import { indexNext, indexPrev } from '#renderer/global/service/utils';
import { SCOUT_DEFAULT_PER_PAGE } from '#renderer/domains/scout/service/researchHelpers';
import type { ScoutCommand } from '#renderer/domains/scout/repo/api';
import type { ScoutItem } from '#renderer/global/types';

export const scoutCommandForStatus = (status: string): ScoutCommand => {
  switch (status) {
    case 'pending':
      return 'mark_pending';
    case 'processed':
      return 'mark_processed';
    case 'saved':
      return 'save';
    case 'archived':
      return 'archive';
    default:
      // invariant: scout status pickers only emit pending, processed, saved, or archived
      throw new Error(`Unsupported scout lifecycle target: ${status}`);
  }
};

interface ScoutPageOptions {
  active: boolean;
  activeItemId: number | null;
  onOpenItem?: (id: number) => void;
  onBackToList?: () => void;
}

export interface ScoutPageState {
  query: ScoutQueryParams;
  items: ScoutItem[];
  page: number;
  pages: number;
  setQuery: (params: Partial<ScoutQueryParams>) => void;
  selectedIds: Set<number>;
  toggleSelect: (id: number) => void;
  clearSelection: () => void;
  view: '' | 'research';
  setView: (v: '' | 'research') => void;
  qaOpen: boolean;
  setQaOpen: Dispatch<SetStateAction<boolean>>;
  qaEverOpened: boolean;
  setQaEverOpened: Dispatch<SetStateAction<boolean>>;
  clampedFocusedIndex: number;
  searchRef: React.RefObject<HTMLInputElement | null>;
  researchModalOpenRef: React.MutableRefObject<boolean>;
  handleBulkStatus: (status: string) => void;
  handleBulkDelete: () => void;
  inListView: boolean;
}

export function useScoutPage({
  active,
  activeItemId,
  onOpenItem,
  onBackToList,
}: ScoutPageOptions): ScoutPageState {
  const [query, setQueryState] = useState<ScoutQueryParams>({
    status: 'all',
    per_page: SCOUT_DEFAULT_PER_PAGE,
  });
  const { data } = useScoutList(query);
  const items: ScoutItem[] = data?.items ?? [];
  const page = data?.page ?? 0;
  const pages = data?.pages ?? 0;
  const setQuery = useCallback((params: Partial<ScoutQueryParams>) => {
    setQueryState((prev) => ({ ...prev, ...params, page: params.page ?? 0 }));
  }, []);

  const scoutFetch = useScoutRefresh();
  const bulkUpdateMut = useScoutBulkUpdate();
  const bulkDeleteMut = useScoutBulkDelete();
  const { selectedIds, toggleSelect, clearSelection } = useSelection();
  const [view, setView] = useState<'' | 'research'>('');
  const [qaOpen, setQaOpen] = useState(false);
  const [qaEverOpened, setQaEverOpened] = useState(false);

  // Sync ephemeral UI state with URL-driven activeItemId. URL state is the
  // source of truth; local state must yield. React permits same-component
  // setState during render for this "store previous props" pattern.
  const prevActiveItemIdRef = useRef(activeItemId);
  if (prevActiveItemIdRef.current !== null && activeItemId === null) {
    if (qaOpen) setQaOpen(false);
    if (qaEverOpened) setQaEverOpened(false);
  }
  if (activeItemId !== null && view !== '') {
    setView('');
  }
  prevActiveItemIdRef.current = activeItemId;

  const [focusedIndex, setFocusedIndex] = useState(-1);
  const searchRef = useRef<HTMLInputElement>(null);

  // researchModalOpen is tracked via ref so the keyboard handler can read it
  // synchronously without adding it to the effect dependency array.
  const researchModalOpenRef = useRef(false);

  // Clamp focusedIndex — derived inline, no effect needed
  const clampedFocusedIndex =
    focusedIndex >= items.length ? (items.length > 0 ? items.length - 1 : -1) : focusedIndex;

  const inListView = view === '' && !activeItemId;

  const handleKey = useCallback(
    (key: string, e: KeyboardEvent) => {
      // The research modal owns the keyboard while it is open — Enter and
      // Escape must submit/cancel the modal, not trigger list actions behind it.
      if (researchModalOpenRef.current) return;
      // Escape from a non-list view returns to the list. Reader path goes
      // through backToList() (URL nav); research view is local useState.
      if (!inListView) {
        if (key === 'Escape') {
          e.preventDefault();
          if (view === 'research') {
            setView('');
          } else {
            onBackToList?.();
          }
        }
        return;
      }
      switch (key) {
        case 'j':
          e.preventDefault();
          setFocusedIndex((i) => indexNext(i, items.length - 1));
          break;
        case 'k':
          e.preventDefault();
          setFocusedIndex((i) => indexPrev(i));
          break;
        case 'Enter': {
          const item = items[clampedFocusedIndex];
          if (item) {
            e.preventDefault();
            onOpenItem?.(item.id);
          }
          break;
        }
        case '/':
          e.preventDefault();
          searchRef.current?.focus();
          break;
        case 'Escape':
          if (clampedFocusedIndex >= 0) {
            e.preventDefault();
            setFocusedIndex(-1);
          }
          break;
      }
    },
    [inListView, items, clampedFocusedIndex, view, onBackToList, onOpenItem],
  );

  useViewKeyHandler(handleKey, active);

  const handleBulkStatus = (status: string) => {
    const ids = [...selectedIds];
    if (!ids.length) return;
    bulkUpdateMut.mutate(
      { ids, command: scoutCommandForStatus(status) },
      {
        onSuccess: () => {
          clearSelection();
          scoutFetch();
        },
      },
    );
  };

  const handleBulkDelete = () => {
    const ids = [...selectedIds];
    if (!ids.length) return;
    bulkDeleteMut.mutate(
      { ids },
      {
        onSuccess: () => {
          clearSelection();
          scoutFetch();
        },
      },
    );
  };

  return {
    query,
    items,
    page,
    pages,
    setQuery,
    selectedIds,
    toggleSelect,
    clearSelection,
    view,
    setView,
    qaOpen,
    setQaOpen,
    qaEverOpened,
    setQaEverOpened,
    clampedFocusedIndex,
    searchRef,
    researchModalOpenRef,
    handleBulkStatus,
    handleBulkDelete,
    inListView,
  };
}
