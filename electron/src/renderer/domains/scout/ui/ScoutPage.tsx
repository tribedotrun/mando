import React, { useCallback, useRef, useState } from 'react';
import {
  useScoutList,
  useScoutRefresh,
  useScoutBulkUpdate,
  useScoutBulkDelete,
  type ScoutQueryParams,
} from '#renderer/domains/scout/runtime/hooks';
import { useViewKeyHandler } from '#renderer/global/runtime/useKeyboardShortcuts';
import { useSelection } from '#renderer/global/runtime/useSelection';
import { BulkBar } from '#renderer/global/ui/BulkBar';
import { ScoutTable } from '#renderer/domains/scout/ui/ScoutTable';
import { ScoutReader } from '#renderer/domains/scout/ui/ScoutReader';
import { ScoutQA } from '#renderer/domains/scout/ui/ScoutQA';
import { ScoutResearch } from '#renderer/domains/scout/ui/ScoutResearch';
import { ScoutListHeader } from '#renderer/domains/scout/ui/ScoutListHeader';
import { ScoutPagination } from '#renderer/domains/scout/ui/ScoutPagination';
import { indexNext, indexPrev } from '#renderer/global/service/utils';
import {
  USER_SETTABLE_STATUSES,
  SCOUT_DEFAULT_PER_PAGE,
} from '#renderer/domains/scout/service/researchHelpers';

interface ScoutPageProps {
  active?: boolean;
  activeItemId?: number | null;
  onOpenItem?: (id: number) => void;
  onBackToList?: () => void;
}

export function ScoutPage({
  active = true,
  activeItemId = null,
  onOpenItem,
  onBackToList,
}: ScoutPageProps): React.ReactElement {
  const [query, setQueryState] = useState<ScoutQueryParams>({
    status: 'all',
    per_page: SCOUT_DEFAULT_PER_PAGE,
  });
  const { data } = useScoutList(query);
  const items = data?.items ?? [];
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
      { ids, updates: { status } },
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

  // URL-driven activeItemId takes priority over local view state.
  if (activeItemId) {
    return (
      <div className="-mx-5 -mb-4 flex" style={{ height: 'calc(100% + 1rem)' }}>
        <div className="flex min-w-0 flex-1 flex-col">
          <ScoutReader
            key={activeItemId}
            itemId={activeItemId}
            onAsk={() => {
              setQaOpen((v) => !v);
              setQaEverOpened(true);
            }}
            qaOpen={qaOpen}
          />
        </div>
        {qaEverOpened && (
          <div className={`w-[380px] shrink-0 bg-card ${qaOpen ? '' : 'hidden'}`}>
            <ScoutQA key={activeItemId} itemId={activeItemId} onClose={() => setQaOpen(false)} />
          </div>
        )}
      </div>
    );
  }

  if (view === 'research') {
    return <ScoutResearch />;
  }

  return (
    <div className="flex flex-col gap-4">
      <ScoutListHeader
        query={query}
        searchRef={searchRef}
        onQueryChange={setQuery}
        onResearchHistoryClick={() => setView('research')}
        onResearchModalOpenChange={(open) => {
          researchModalOpenRef.current = open;
        }}
      />

      <ScoutTable
        items={items}
        selectedIds={selectedIds}
        callbacks={{
          onToggleSelect: toggleSelect,
          onSelect: (id) => onOpenItem?.(id),
        }}
        focusedIndex={clampedFocusedIndex}
      />

      <ScoutPagination page={page} pages={pages} onPageChange={(p) => setQuery({ page: p })} />

      <BulkBar
        count={selectedIds.size}
        statuses={USER_SETTABLE_STATUSES as unknown as string[]}
        onDelete={handleBulkDelete}
        onBulkStatus={handleBulkStatus}
        onCancel={clearSelection}
      />
    </div>
  );
}
