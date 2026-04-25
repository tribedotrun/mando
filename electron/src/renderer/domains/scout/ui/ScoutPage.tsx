import React from 'react';
import { useScoutPage } from '#renderer/domains/scout/runtime/useScoutPage';
import { SelectionToast } from '#renderer/domains/scout/ui/SelectionToast';
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
  const page = useScoutPage({ active, activeItemId, onOpenItem, onBackToList });

  // URL-driven activeItemId takes priority over local view state.
  if (activeItemId) {
    return (
      <div className="-mx-5 -mb-4 flex" style={{ height: 'calc(100% + 1rem)' }}>
        <div className="flex min-w-0 flex-1 flex-col">
          <ScoutReader
            key={activeItemId}
            itemId={activeItemId}
            onAsk={() => {
              page.qa.setOpen((v) => !v);
              page.qa.setEverOpened(true);
            }}
            qaOpen={page.qa.open}
          />
        </div>
        {page.qa.everOpened && (
          <div className={`w-[380px] shrink-0 bg-card ${page.qa.open ? '' : 'hidden'}`}>
            <ScoutQA
              key={activeItemId}
              itemId={activeItemId}
              onClose={() => page.qa.setOpen(false)}
            />
          </div>
        )}
      </div>
    );
  }

  if (page.list.view === 'research') {
    return <ScoutResearch />;
  }

  return (
    <div className="flex flex-col gap-4">
      <ScoutListHeader
        query={page.query.params}
        searchRef={page.focus.searchRef}
        onQueryChange={page.query.set}
        onResearchHistoryClick={() => page.list.setView('research')}
        onResearchModalOpenChange={(open) => {
          page.focus.researchModalOpenRef.current = open;
        }}
      />

      <ScoutTable
        items={page.list.items}
        selectedIds={page.selection.selectedIds}
        callbacks={{
          onToggleSelect: page.selection.toggleSelect,
          onSelect: (id) => onOpenItem?.(id),
        }}
        focusedIndex={page.focus.clampedIndex}
      />

      <ScoutPagination
        page={page.query.page}
        pages={page.query.pages}
        onPageChange={(p) => page.query.set({ page: p })}
      />

      <SelectionToast
        count={page.selection.selectedIds.size}
        statuses={USER_SETTABLE_STATUSES}
        onDelete={page.actions.handleBulkDelete}
        onBulkStatus={page.actions.handleBulkStatus}
        onCancel={page.selection.clearSelection}
      />
    </div>
  );
}
