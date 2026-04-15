import React, { useCallback, useRef, useState } from 'react';
import { Filter, History, Search } from 'lucide-react';
import { useScoutList, type ScoutQueryParams } from '#renderer/hooks/queries';
import { useQueryClient } from '@tanstack/react-query';
import { queryKeys } from '#renderer/queryKeys';
import { useViewKeyHandler } from '#renderer/global/hooks/useKeyboardShortcuts';
import { useSelection, BulkBar, FeedbackModal } from '#renderer/domains/captain';
import { ScoutTable } from '#renderer/domains/scout/components/ScoutTable';
import { AddUrlForm } from '#renderer/domains/scout/components/AddUrlForm';
import { ScoutFilterMenu } from '#renderer/domains/scout/components/ScoutFilterMenu';
import { ScoutReader } from '#renderer/domains/scout/components/ScoutReader';
import { ScoutQA } from '#renderer/domains/scout/components/ScoutQA';
import { ScoutResearch } from '#renderer/domains/scout/components/ScoutResearch';
import {
  bulkUpdateScout,
  bulkDeleteScout,
  researchScout,
} from '#renderer/domains/scout/hooks/useApi';
import { toast } from 'sonner';
import { getErrorMessage, indexNext, indexPrev } from '#renderer/utils';
import { Button } from '#renderer/components/ui/button';
import { Input } from '#renderer/components/ui/input';

const USER_SETTABLE = ['pending', 'processed', 'saved', 'archived'];
const SEARCH_DEBOUNCE_MS = 300;
const DEFAULT_PER_PAGE = 25;

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
    per_page: DEFAULT_PER_PAGE,
  });
  const queryClient = useQueryClient();
  const { data } = useScoutList(query);
  const items = data?.items ?? [];
  const page = data?.page ?? 0;
  const pages = data?.pages ?? 0;
  const setQuery = useCallback((params: Partial<ScoutQueryParams>) => {
    setQueryState((prev) => ({ ...prev, ...params, page: params.page ?? 0 }));
  }, []);

  const scoutFetch = useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: queryKeys.scout.all });
  }, [queryClient]);
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
  const [searchInput, setSearchInput] = useState('');
  const [focusedIndex, setFocusedIndex] = useState(-1);
  const [filterOpen, setFilterOpen] = useState(false);
  const [researchModalOpen, setResearchModalOpen] = useState(false);
  const [researchPending, setResearchPending] = useState(false);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const searchRef = useRef<HTMLInputElement>(null);

  // Clamp focusedIndex — derived inline, no effect needed
  const clampedFocusedIndex =
    focusedIndex >= items.length ? (items.length > 0 ? items.length - 1 : -1) : focusedIndex;

  const inListView = view === '' && !activeItemId;

  const runResearch = useCallback(async (topic: string) => {
    setResearchPending(true);
    try {
      await researchScout(topic, true);
      toast.success('Research started');
      setResearchModalOpen(false);
    } catch (err) {
      toast.error(getErrorMessage(err, 'Research failed'));
    } finally {
      setResearchPending(false);
    }
  }, []);

  const handleKey = useCallback(
    (key: string, e: KeyboardEvent) => {
      // The research modal owns the keyboard while it is open — Enter and
      // Escape must submit/cancel the modal, not trigger list actions behind it.
      if (researchModalOpen) return;
      // Escape from a non-list view returns to the list. Reader path goes
      // through backToList() (URL nav); research view is local useState.
      if (!inListView) {
        if (key === 'Escape') {
          e.preventDefault();
          if (view === 'research') {
            setView('');
          } else {
            backToList();
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
            openReader(item.id);
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
    [inListView, items, clampedFocusedIndex, researchModalOpen, view],
  );

  useViewKeyHandler(handleKey, active);

  const statusFilter = query.status ?? 'all';
  const typeFilter = query.type ?? 'all';

  const handleSearchChange = useCallback(
    (value: string) => {
      setSearchInput(value);
      clearTimeout(debounceRef.current);
      debounceRef.current = setTimeout(() => {
        setQuery({ q: value || undefined, page: 0 });
      }, SEARCH_DEBOUNCE_MS);
    },
    [setQuery],
  );

  const handleStatusChange = useCallback(
    (status: string) => {
      setQuery({ status: status === 'all' ? 'all' : status, page: 0 });
    },
    [setQuery],
  );

  const handleTypeChange = useCallback(
    (type: string) => {
      setQuery({ type: type === 'all' ? undefined : type, page: 0 });
    },
    [setQuery],
  );

  const handlePageChange = useCallback(
    (p: number) => {
      setQuery({ page: p });
    },
    [setQuery],
  );

  const runBulkAction = async (action: (ids: number[]) => Promise<unknown>) => {
    const ids = [...selectedIds];
    if (!ids.length) return;
    try {
      await action(ids);
      clearSelection();
      scoutFetch();
    } catch (err) {
      toast.error(getErrorMessage(err, 'Bulk scout action failed'));
    }
  };

  const handleBulkStatus = (status: string) => {
    void runBulkAction((ids) => bulkUpdateScout(ids, { status }));
  };

  const handleBulkDelete = () => {
    void runBulkAction(bulkDeleteScout);
  };

  const openReader = (id: number) => {
    onOpenItem?.(id);
  };
  const backToList = () => {
    onBackToList?.();
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
      {/* Row 1: Title + actions */}
      <div className="flex items-baseline justify-between">
        <h2 className="text-heading text-foreground">Scout</h2>
        <div className="flex items-center gap-2">
          <ScoutFilterMenu
            typeValue={typeFilter}
            stateValue={statusFilter}
            onTypeChange={handleTypeChange}
            onStateChange={handleStatusChange}
            open={filterOpen}
            onOpenChange={setFilterOpen}
          >
            <Button variant="ghost" size="icon-sm" className="relative" aria-label="Filter">
              <Filter size={16} />
              {(typeFilter !== 'all' || statusFilter !== 'all') && (
                <span className="absolute right-1 top-1 size-1.5 rounded-full bg-foreground" />
              )}
            </Button>
          </ScoutFilterMenu>
          <Button
            variant="outline"
            size="sm"
            onClick={() => setResearchModalOpen(true)}
            disabled={researchPending}
          >
            {researchPending ? 'Researching...' : 'Research'}
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={() => setView('research')}
            title="Research history"
            aria-label="Open research history"
          >
            <History size={16} />
          </Button>
          <AddUrlForm />
        </div>
      </div>

      {/* Row 2: Search */}
      <div className="relative flex items-center gap-3">
        <Search
          size={14}
          className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground"
          strokeWidth={1.5}
        />
        <Input
          ref={searchRef}
          type="text"
          value={searchInput}
          onChange={(e) => handleSearchChange(e.target.value)}
          placeholder="Search..."
          className="h-9 pl-8 text-[13px]"
        />
      </div>

      <ScoutTable
        items={items}
        selectedIds={selectedIds}
        callbacks={{
          onToggleSelect: toggleSelect,
          onSelect: openReader,
          onRefresh: () => void scoutFetch(),
        }}
        focusedIndex={clampedFocusedIndex}
      />

      {/* Pagination */}
      {pages > 1 && (
        <div className="flex items-center justify-center gap-2 pt-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => handlePageChange(page - 1)}
            disabled={page === 0}
          >
            Prev
          </Button>
          <span className="text-code tabular-nums text-muted-foreground">
            {page + 1} / {pages}
          </span>
          <Button
            variant="outline"
            size="sm"
            onClick={() => handlePageChange(page + 1)}
            disabled={page >= pages - 1}
          >
            Next
          </Button>
        </div>
      )}

      <BulkBar
        count={selectedIds.size}
        statuses={USER_SETTABLE}
        onDelete={handleBulkDelete}
        onBulkStatus={handleBulkStatus}
        onCancel={clearSelection}
      />

      {researchModalOpen && (
        <FeedbackModal
          testId="scout-research-modal"
          title="Scout research"
          placeholder="What should Scout research? (e.g. Rust async runtime fairness)"
          buttonLabel="Research"
          pendingLabel="Researching..."
          isPending={researchPending}
          onSubmit={(topic) => {
            void runResearch(topic);
          }}
          onCancel={() => setResearchModalOpen(false)}
        />
      )}
    </div>
  );
}
