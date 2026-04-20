import React from 'react';
import { Filter, History, Search } from 'lucide-react';
import { useScoutListHeader } from '#renderer/domains/scout/runtime/useScoutListHeader';
import type { ScoutQueryParams } from '#renderer/domains/scout/runtime/hooks';
import { ScoutFilterMenu } from '#renderer/domains/scout/ui/ScoutFilterMenu';
import { AddUrlForm } from '#renderer/domains/scout/ui/AddUrlForm';
import { FeedbackModal } from '#renderer/global/ui/FeedbackModal';
import { Button } from '#renderer/global/ui/button';
import { Input } from '#renderer/global/ui/input';

interface Props {
  query: ScoutQueryParams;
  searchRef: React.RefObject<HTMLInputElement | null>;
  onQueryChange: (params: Partial<ScoutQueryParams>) => void;
  onResearchHistoryClick: () => void;
  onResearchModalOpenChange: (open: boolean) => void;
}

export function ScoutListHeader({
  query,
  searchRef,
  onQueryChange,
  onResearchHistoryClick,
  onResearchModalOpenChange,
}: Props): React.ReactElement {
  const {
    searchInput,
    filterOpen,
    setFilterOpen,
    researchModalOpen,
    researchPending,
    setResearchOpen,
    runResearch,
    handleSearchChange,
    handleStatusChange,
    handleTypeChange,
  } = useScoutListHeader({ onQueryChange, onResearchModalOpenChange });

  const statusFilter = query.status ?? 'all';
  const typeFilter = query.type ?? 'all';

  return (
    <>
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
            onClick={() => setResearchOpen(true)}
            disabled={researchPending}
          >
            {researchPending ? 'Researching...' : 'Research'}
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={onResearchHistoryClick}
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

      {researchModalOpen && (
        <FeedbackModal
          testId="scout-research-modal"
          title="Scout research"
          placeholder="What should Scout research? (e.g. Rust async runtime fairness)"
          buttonLabel="Research"
          pendingLabel="Researching..."
          isPending={researchPending}
          onSubmit={runResearch}
          onCancel={() => setResearchOpen(false)}
        />
      )}
    </>
  );
}
