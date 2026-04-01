import { create } from 'zustand';
import type { ScoutItem } from '#renderer/types';
import { fetchScoutItems, addScoutUrl, deleteScoutItem } from '#renderer/api';
import { getErrorMessage } from '#renderer/utils';
import type { ScoutQueryParams } from '#renderer/api';

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
  remove: (id: number) => Promise<void>;
}

const DEFAULT_PER_PAGE = 25;

let fetchGeneration = 0;

export const useScoutStore = create<ScoutStore>((set, getState) => {
  /** Run a mutation, re-fetch on success, set error + rethrow on failure. */
  async function mutate(fn: () => Promise<unknown>, errLabel: string): Promise<void> {
    try {
      await fn();
      await getState().fetch();
    } catch (err) {
      set({ error: getErrorMessage(err, errLabel) });
      throw err;
    }
  }

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
      const gen = ++fetchGeneration;
      set({ loading: true, error: null, query: merged });
      try {
        const data = await fetchScoutItems(merged);
        if (gen !== fetchGeneration) return; // stale response
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
        if (gen !== fetchGeneration) return;
        set({ loading: false, error: getErrorMessage(err, 'Failed to fetch scout items') });
      }
    },

    setQuery: (params: Partial<ScoutQueryParams>) => {
      const current = getState().query;
      const merged = { ...current, ...params, page: params.page ?? 0 };
      getState().fetch(merged);
    },

    add: (url, title) => mutate(() => addScoutUrl(url, title), 'Failed to add URL'),
    remove: (id) => mutate(() => deleteScoutItem(id), 'Failed to delete scout item'),
  };
});
