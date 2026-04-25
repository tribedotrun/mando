import { useCallback, useState } from 'react';
import { useScoutResearch, type ScoutQueryParams } from '#renderer/domains/scout/runtime/hooks';
import { useDebouncedCallback } from '#renderer/domains/scout/runtime/useDebouncedCallback';
import type { ScoutStatusFilter } from '#renderer/domains/scout/service/researchHelpers';

const SEARCH_DEBOUNCE_MS = 300;

interface UseScoutListHeaderOptions {
  onQueryChange: (params: Partial<ScoutQueryParams>) => void;
  onResearchModalOpenChange: (open: boolean) => void;
}

export interface ScoutListHeaderState {
  search: {
    input: string;
    handleChange: (value: string) => void;
  };
  filter: {
    open: boolean;
    setOpen: (v: boolean) => void;
    handleStatusChange: (status: ScoutStatusFilter) => void;
    handleTypeChange: (type: string) => void;
  };
  research: {
    open: boolean;
    pending: boolean;
    setOpen: (open: boolean) => void;
    run: (topic: string) => Promise<void>;
  };
}

export function useScoutListHeader({
  onQueryChange,
  onResearchModalOpenChange,
}: UseScoutListHeaderOptions): ScoutListHeaderState {
  const [searchInput, setSearchInput] = useState('');
  const [filterOpen, setFilterOpen] = useState(false);
  const [researchModalOpen, setResearchModalOpen] = useState(false);
  const researchMut = useScoutResearch();

  const setResearchOpen = useCallback(
    (open: boolean) => {
      setResearchModalOpen(open);
      onResearchModalOpenChange(open);
    },
    [onResearchModalOpenChange],
  );

  const runResearch = useCallback(
    async (topic: string) => {
      await researchMut.mutateAsync({ topic });
      setResearchOpen(false);
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
    (status: ScoutStatusFilter) => {
      onQueryChange({ status, page: 0 });
    },
    [onQueryChange],
  );

  const handleTypeChange = useCallback(
    (type: string) => {
      onQueryChange({ type: type === 'all' ? undefined : type, page: 0 });
    },
    [onQueryChange],
  );

  return {
    search: { input: searchInput, handleChange: handleSearchChange },
    filter: {
      open: filterOpen,
      setOpen: setFilterOpen,
      handleStatusChange,
      handleTypeChange,
    },
    research: {
      open: researchModalOpen,
      pending: researchMut.isPending,
      setOpen: setResearchOpen,
      run: runResearch,
    },
  };
}
