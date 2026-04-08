import { create } from 'zustand';
import type { ScoutItem } from '#renderer/types';
import { fetchScoutItems, addScoutUrl, type ScoutQueryParams } from '#renderer/api';
import { createMutate, getErrorMessage } from '#renderer/utils';
import { createFetchGenerationGuard } from '#renderer/global/stores/utils';

interface ScoutStore {
  items: ScoutItem[];
  total: number;
  page: number;
  pages: number;
  perPage: number;
  statusCounts: Record<string, number>;
  loading: boolean;
  error: string | null;
  query: ScoutQueryParams;
  fetch: (params?: ScoutQueryParams) => Promise<void>;
  setQuery: (params: Partial<ScoutQueryParams>) => void;
  add: (url: string, title?: string) => Promise<void>;
}

const DEFAULT_PER_PAGE = 25;

const fetchGen = createFetchGenerationGuard();

export const useScoutStore = create<ScoutStore>((set, getState) => {
  const mutate = createMutate(getState, set);

  return {
    items: [],
    total: 0,
    page: 0,
    pages: 0,
    perPage: DEFAULT_PER_PAGE,
    statusCounts: {},
    loading: false,
    error: null,
    query: { status: 'all', per_page: DEFAULT_PER_PAGE },

    fetch: async (params?: ScoutQueryParams) => {
      const merged = { ...getState().query, ...params };
      const gen = fetchGen.next();
      set({ loading: true, error: null, query: merged });
      try {
        const data = await fetchScoutItems(merged);
        if (!fetchGen.isLatest(gen)) return; // stale response
        set({
          items: data.items,
          total: data.total,
          page: data.page,
          pages: data.pages,
          perPage: data.per_page,
          statusCounts: data.status_counts ?? {},
          loading: false,
        });
      } catch (err) {
        if (!fetchGen.isLatest(gen)) return;
        set({ loading: false, error: getErrorMessage(err, 'Failed to fetch scout items') });
      }
    },

    setQuery: (params: Partial<ScoutQueryParams>) => {
      const current = getState().query;
      const merged = { ...current, ...params, page: params.page ?? 0 };
      void getState().fetch(merged);
    },

    add: (url, title) => mutate(() => addScoutUrl(url, title), 'Failed to add URL'),
  };
});
