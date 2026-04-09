import { QueryClient, type QueryClient as QC } from '@tanstack/react-query';
import { queryKeys } from '#renderer/queryKeys';

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5_000,
      // Read queries retry up to 2 times with exponential backoff so a
      // transient daemon hiccup doesn't surface as a hard error to the user.
      retry: 2,
      retryDelay: (attemptIndex) => Math.min(1000 * 2 ** attemptIndex, 10_000),
      refetchOnWindowFocus: false,
    },
    mutations: {
      // Mutations never retry automatically to avoid duplicating side effects.
      retry: 0,
    },
  },
});

/**
 * Invalidate the query caches that back the task detail view
 * (timeline + PR summary + Q&A history).
 * Pass `id` to scope to a single task, or omit to invalidate for all tasks.
 */
export function invalidateTaskDetail(client: QC, id?: number): void {
  if (id != null) {
    void client.invalidateQueries({ queryKey: queryKeys.tasks.timeline(id) });
    void client.invalidateQueries({ queryKey: queryKeys.tasks.pr(id) });
    void client.invalidateQueries({ queryKey: queryKeys.tasks.askHistory(id) });
  } else {
    // Invalidate all task sub-queries (timeline, pr, ask-history for every task)
    void client.invalidateQueries({ queryKey: queryKeys.tasks.all });
  }
}
