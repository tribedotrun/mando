import { useCallback, useState } from 'react';
import { useScoutResearch, type ScoutQueryParams } from '#renderer/domains/scout/runtime/hooks';
import { useDebouncedCallback } from '#renderer/domains/scout/runtime/useDebouncedCallback';

const SEARCH_DEBOUNCE_MS = 300;

interface UseScoutListHeaderOptions {
  onQueryChange: (params: Partial<ScoutQueryParams>) => void;
  onResearchModalOpenChange: (open: boolean) => void;
}

export interface ScoutListHeaderState {
  searchInput: string;
  filterOpen: boolean;
  setFilterOpen: (v: boolean) => void;
  researchModalOpen: boolean;
  researchPending: boolean;
  setResearchOpen: (open: boolean) => void;
  runResearch: (topic: string) => void;
  handleSearchChange: (value: string) => void;
  handleStatusChange: (status: string) => void;
  handleTypeChange: (type: string) => void;
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

  return {
    searchInput,
    filterOpen,
    setFilterOpen,
    researchModalOpen,
    researchPending: researchMut.isPending,
    setResearchOpen,
    runResearch,
    handleSearchChange,
    handleStatusChange,
    handleTypeChange,
  };
}
