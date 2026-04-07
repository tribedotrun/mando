import React, { useCallback, useRef, useState } from 'react';
import { Search } from 'lucide-react';
import { useScoutStore } from '#renderer/domains/scout/stores/scoutStore';
import { useViewKeyHandler } from '#renderer/global/hooks/useKeyboardShortcuts';
import { useSelection } from '#renderer/domains/captain';
import { ScoutTable } from '#renderer/domains/scout/components/ScoutTable';
import { AddUrlForm } from '#renderer/domains/scout/components/AddUrlForm';
import { ScoutStatusTabs } from '#renderer/domains/scout/components/ScoutStatusTabs';
import { ScoutReader } from '#renderer/domains/scout/components/ScoutReader';
import { ScoutQA } from '#renderer/domains/scout/components/ScoutQA';
import { BulkBar, FeedbackModal } from '#renderer/domains/captain';
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
const TYPES = ['all', 'github', 'youtube', 'arxiv', 'other'] as const;

export function ScoutPage({ active = true }: { active?: boolean } = {}): React.ReactElement {
  const {
    query,
    setQuery,
    items,
    total,
    page,
    pages,
    statusCounts,
    fetch: scoutFetch,
  } = useScoutStore();
  const { selectedIds, toggleSelect, clearSelection } = useSelection();
  const [activeItemId, setActiveItemId] = useState<number | null>(null);
  const [view, setView] = useState<'' | 'read'>('');
  const [qaOpen, setQaOpen] = useState(false);
  const [qaEverOpened, setQaEverOpened] = useState(false);
  const [searchInput, setSearchInput] = useState('');
  const [focusedIndex, setFocusedIndex] = useState(-1);
  const [researchModalOpen, setResearchModalOpen] = useState(false);
  const [researchPending, setResearchPending] = useState(false);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const searchRef = useRef<HTMLInputElement>(null);

  // Clamp focusedIndex — derived inline, no effect needed
  const clampedFocusedIndex =
    focusedIndex >= items.length ? (items.length > 0 ? items.length - 1 : -1) : focusedIndex;

  const inListView = view === '' && !activeItemId;

  const runResearch = useCallback(
    async (topic: string) => {
      setResearchPending(true);
      try {
        const result = await researchScout(topic, true);
        await scoutFetch();
        const added = result.added ?? 0;
        const processed = result.processed ?? 0;
        toast.success(`Research added ${added} link(s) and processed ${processed}`);
        setResearchModalOpen(false);
      } catch (err) {
        toast.error(getErrorMessage(err, 'Research failed'));
      } finally {
        setResearchPending(false);
      }
    },
    [scoutFetch],
  );

  const handleKey = useCallback(
    (key: string, e: KeyboardEvent) => {
      // The research modal owns the keyboard while it is open — Enter and
      // Escape must submit/cancel the modal, not trigger list actions behind it.
      if (researchModalOpen) return;
      // Escape from reader goes back to list
      if (!inListView) {
        if (key === 'Escape') {
          e.preventDefault();
          backToList();
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
    [inListView, items, clampedFocusedIndex, researchModalOpen],
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
      }, 300);
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
      await scoutFetch();
    } catch (err) {
      toast.error(getErrorMessage(err, 'Bulk scout action failed'));
    }
  };

  const handleBulkStatus = (status: string) =>
    runBulkAction((ids) => bulkUpdateScout(ids, { status }));

  const handleBulkDelete = () => runBulkAction(bulkDeleteScout);

  const openReader = (id: number) => {
    setActiveItemId(id);
    setView('read');
  };
  const backToList = () => {
    setActiveItemId(null);
    setView('');
    setQaOpen(false);
    setQaEverOpened(false);
  };

  if (view === 'read' && activeItemId) {
    return (
      <div className="-mx-5 -mb-4 flex" style={{ height: 'calc(100% + 1rem)' }}>
        <div className="min-w-0 flex-1 overflow-y-auto px-5 py-4">
          <ScoutReader
            key={activeItemId}
            itemId={activeItemId}
            onBack={backToList}
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

  return (
    <div className="flex flex-col gap-4">
      {/* Row 1: Title + count */}
      <div className="flex items-baseline justify-between">
        <div className="flex items-baseline gap-3">
          <h2 className="text-heading text-foreground">Scout</h2>
          <span className="text-caption text-text-3">{total} items</span>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => setResearchModalOpen(true)}
            disabled={researchPending}
          >
            {researchPending ? 'Researching...' : 'Research'}
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

      {/* Row 3: Type filter */}
      <div className="flex items-center gap-1">
        {TYPES.map((t) => {
          const isActive = typeFilter === t;
          return (
            <Button
              key={t}
              variant={isActive ? 'default' : 'secondary'}
              size="xs"
              onClick={() => handleTypeChange(t)}
            >
              {t}
            </Button>
          );
        })}
      </div>

      <ScoutStatusTabs
        activeStatus={statusFilter}
        onStatusChange={handleStatusChange}
        statusCounts={statusCounts}
      />

      <ScoutTable
        items={items}
        selectedIds={selectedIds}
        onToggleSelect={toggleSelect}
        onSelect={openReader}
        onRefresh={() => scoutFetch()}
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
