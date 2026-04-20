import React from 'react';
import { useScoutPage } from '#renderer/domains/scout/runtime/useScoutPage';
import { BulkBar } from '#renderer/global/ui/BulkBar';
import { ScoutTable } from '#renderer/domains/scout/ui/ScoutTable';
import { ScoutReader } from '#renderer/domains/scout/ui/ScoutReader';
import { ScoutQA } from '#renderer/domains/scout/ui/ScoutQA';
import { ScoutResearch } from '#renderer/domains/scout/ui/ScoutResearch';
import { ScoutListHeader } from '#renderer/domains/scout/ui/ScoutListHeader';
import { ScoutPagination } from '#renderer/domains/scout/ui/ScoutPagination';
import { USER_SETTABLE_STATUSES } from '#renderer/domains/scout/service/researchHelpers';

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
  const {
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
  } = useScoutPage({ active, activeItemId, onOpenItem, onBackToList });

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
