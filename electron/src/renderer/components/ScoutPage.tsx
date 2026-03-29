import React, { useCallback, useRef, useState } from 'react';
import { useScoutStore } from '#renderer/stores/scoutStore';
import { useViewKeyHandler } from '#renderer/hooks/useKeyboardShortcuts';
import { useSelection } from '#renderer/hooks/useSelection';
import { ScoutTable } from '#renderer/components/ScoutTable';
import { AddUrlForm } from '#renderer/components/AddUrlForm';
import { ScoutStatusTabs } from '#renderer/components/ScoutStatusTabs';
import { ScoutActions } from '#renderer/components/ScoutActions';
import { ScoutReader } from '#renderer/components/ScoutReader';
import { ScoutQA } from '#renderer/components/ScoutQA';
import { BulkBar } from '#renderer/components/BulkBar';
import { bulkUpdateScout, bulkDeleteScout } from '#renderer/api';
import { useToastStore } from '#renderer/stores/toastStore';

const USER_SETTABLE = ['pending', 'processed', 'saved', 'archived'];
const TYPES = ['all', 'github', 'youtube', 'arxiv', 'other'] as const;

export function ScoutPage(): React.ReactElement {
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
  const { selectedIds, toggleSelect, toggleSelectAll, clearSelection } = useSelection();
  const [activeItemId, setActiveItemId] = useState<number | null>(null);
  const [view, setView] = useState<'' | 'read'>('');
  const [qaOpen, setQaOpen] = useState(false);
  const [qaEverOpened, setQaEverOpened] = useState(false);
  const [searchInput, setSearchInput] = useState('');
  const [focusedIndex, setFocusedIndex] = useState(-1);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const searchRef = useRef<HTMLInputElement>(null);

  // Clamp focusedIndex — derived inline, no effect needed
  const clampedFocusedIndex =
    focusedIndex >= items.length ? (items.length > 0 ? items.length - 1 : -1) : focusedIndex;

  const inListView = view === '' && !activeItemId;

  const handleKey = useCallback(
    (key: string, e: KeyboardEvent) => {
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
          setFocusedIndex((i) => Math.min(i + 1, items.length - 1));
          break;
        case 'k':
          e.preventDefault();
          setFocusedIndex((i) => Math.max(i - 1, 0));
          break;
        case 'Enter': {
          const item = items[clampedFocusedIndex];
          if (item) {
            e.preventDefault();
            openReader(item.id);
          }
          break;
        }
        case 't':
          e.preventDefault();
          // ScoutActions "Process" button triggers processScout on pending items
          // Navigate to pending by setting status filter
          setQuery({ status: 'pending', page: 0 });
          break;
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
    [inListView, items, clampedFocusedIndex, setQuery],
  );

  useViewKeyHandler(handleKey);

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

  const handleBulkStatus = async (status: string) => {
    const ids = [...selectedIds];
    if (!ids.length) return;
    try {
      await bulkUpdateScout(ids, { status });
      clearSelection();
      scoutFetch();
    } catch (err) {
      useToastStore
        .getState()
        .add('error', `Failed: ${err instanceof Error ? err.message : String(err)}`);
    }
  };

  const handleBulkDelete = async () => {
    const ids = [...selectedIds];
    if (!ids.length) return;
    try {
      await bulkDeleteScout(ids);
      clearSelection();
      scoutFetch();
    } catch (err) {
      useToastStore
        .getState()
        .add('error', `Failed: ${err instanceof Error ? err.message : String(err)}`);
    }
  };

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
          <div
            className={`w-[380px] shrink-0 ${qaOpen ? '' : 'hidden'}`}
            style={{
              borderLeft: '1px solid var(--color-border)',
              background: 'var(--color-surface-1)',
            }}
          >
            <ScoutQA key={activeItemId} itemId={activeItemId} onClose={() => setQaOpen(false)} />
          </div>
        )}
      </div>
    );
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      {/* Row 1: Title + count */}
      <div className="flex items-baseline justify-between">
        <div className="flex items-baseline gap-3">
          <h2 className="text-heading" style={{ color: 'var(--color-text-1)' }}>
            Scout
          </h2>
          <span className="text-caption" style={{ color: 'var(--color-text-3)' }}>
            {total} items
          </span>
        </div>
        <AddUrlForm />
      </div>

      {/* Row 2: Process CTA + search */}
      <div className="flex items-center gap-3">
        <ScoutActions onDone={() => scoutFetch()} />
        <div className="flex flex-1 items-center gap-2" style={{ position: 'relative' }}>
          <svg
            width="14"
            height="14"
            viewBox="0 0 16 16"
            fill="none"
            stroke="var(--color-text-3)"
            strokeWidth="1.5"
            style={{
              position: 'absolute',
              left: 10,
              top: '50%',
              transform: 'translateY(-50%)',
              pointerEvents: 'none',
            }}
          >
            <circle cx="7" cy="7" r="4.5" />
            <path d="M10.5 10.5L14 14" strokeLinecap="round" />
          </svg>
          <input
            ref={searchRef}
            type="text"
            value={searchInput}
            onChange={(e) => handleSearchChange(e.target.value)}
            placeholder="Search..."
            className="w-full rounded-md text-[13px] focus:outline-none"
            style={{
              background: 'var(--color-surface-2)',
              color: 'var(--color-text-1)',
              border: '1px solid var(--color-border-subtle)',
              padding: '6px 12px 6px 30px',
            }}
          />
        </div>
      </div>

      {/* Row 3: Type filter */}
      <div className="flex items-center" style={{ gap: 4 }}>
        {TYPES.map((t) => {
          const active = typeFilter === t;
          return (
            <button
              key={t}
              onClick={() => handleTypeChange(t)}
              className="text-[12px] font-medium transition-colors"
              style={{
                background: active ? 'var(--color-accent)' : 'var(--color-surface-2)',
                color: active ? 'var(--color-bg)' : 'var(--color-text-3)',
                padding: '4px 10px',
                borderRadius: 6,
                border: 'none',
                cursor: 'pointer',
              }}
            >
              {t}
            </button>
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
        onToggleSelectAll={() => toggleSelectAll(items)}
        onSelect={openReader}
        onRefresh={() => scoutFetch()}
        focusedIndex={clampedFocusedIndex}
      />

      {/* Pagination */}
      {pages > 1 && (
        <div className="flex items-center justify-center gap-2 pt-2">
          <button
            onClick={() => handlePageChange(page - 1)}
            disabled={page === 0}
            className="rounded-md px-2.5 py-1 text-[13px] disabled:opacity-30"
            style={{ border: '1px solid var(--color-border)', color: 'var(--color-text-2)' }}
          >
            Prev
          </button>
          <span className="text-code tabular-nums" style={{ color: 'var(--color-text-3)' }}>
            {page + 1} / {pages}
          </span>
          <button
            onClick={() => handlePageChange(page + 1)}
            disabled={page >= pages - 1}
            className="rounded-md px-2.5 py-1 text-[13px] disabled:opacity-30"
            style={{ border: '1px solid var(--color-border)', color: 'var(--color-text-2)' }}
          >
            Next
          </button>
        </div>
      )}

      <BulkBar
        count={selectedIds.size}
        statuses={USER_SETTABLE}
        onDelete={handleBulkDelete}
        onBulkStatus={handleBulkStatus}
        onCancel={clearSelection}
      />
    </div>
  );
}
