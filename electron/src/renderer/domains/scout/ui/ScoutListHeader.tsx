import React, { useCallback, useState } from 'react';
import { Filter, History, Search } from 'lucide-react';
import { useScoutResearch, type ScoutQueryParams } from '#renderer/domains/scout/runtime/hooks';
import { useDebouncedCallback } from '#renderer/domains/scout/runtime/useDebouncedCallback';
import { ScoutFilterMenu } from '#renderer/domains/scout/ui/ScoutFilterMenu';
import { AddUrlForm } from '#renderer/domains/scout/ui/AddUrlForm';
import { FeedbackModal } from '#renderer/global/ui/FeedbackModal';
import { Button } from '#renderer/global/ui/button';
import { Input } from '#renderer/global/ui/input';

const SEARCH_DEBOUNCE_MS = 300;

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
  const [searchInput, setSearchInput] = useState('');
  const [filterOpen, setFilterOpen] = useState(false);
  const [researchModalOpen, setResearchModalOpen] = useState(false);
  const researchMut = useScoutResearch();

  const statusFilter = query.status ?? 'all';
  const typeFilter = query.type ?? 'all';

  const setResearchOpen = useCallback(
    (open: boolean) => {
      setResearchModalOpen(open);
      onResearchModalOpenChange(open);
    },
    [onResearchModalOpenChange],
  );

  const runResearch = useCallback(
    (topic: string) => {
      researchMut.mutate({ topic }, { onSuccess: () => setResearchOpen(false) });
    },
    [researchMut, setResearchOpen],
  );

  const debouncedQueryChange = useDebouncedCallback(
    (value: string) => onQueryChange({ q: value || undefined, page: 0 }),
    SEARCH_DEBOUNCE_MS,
  );

  const handleSearchChange = useCallback(
    (value: string) => {
      setSearchInput(value);
      debouncedQueryChange(value);
    },
    [debouncedQueryChange],
  );

  const handleStatusChange = useCallback(
    (status: string) => {
      onQueryChange({ status: status === 'all' ? 'all' : status, page: 0 });
    },
    [onQueryChange],
  );

  const handleTypeChange = useCallback(
    (type: string) => {
      onQueryChange({ type: type === 'all' ? undefined : type, page: 0 });
    },
    [onQueryChange],
  );

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
            disabled={researchMut.isPending}
          >
            {researchMut.isPending ? 'Researching...' : 'Research'}
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
          isPending={researchMut.isPending}
          onSubmit={runResearch}
          onCancel={() => setResearchOpen(false)}
        />
      )}
    </>
  );
}
