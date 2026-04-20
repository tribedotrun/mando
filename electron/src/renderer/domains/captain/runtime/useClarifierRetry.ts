import { useQueryClient } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';

// Returns a Promise so `RetryButton` can reset its `retrying` state when
// the invalidation settles. A void-returning callback leaves the button
// permanently disabled showing "Refreshing…" until the component
// unmounts (devin + codex reviews on PR #886).
export function useClarifierRetry(taskId: number): () => Promise<void> {
  const queryClient = useQueryClient();
  return async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: queryKeys.tasks.all }),
      queryClient.invalidateQueries({
        queryKey: queryKeys.tasks.timeline(taskId),
      }),
      queryClient.invalidateQueries({
        queryKey: queryKeys.tasks.feed(taskId),
      }),
    ]);
  };
}
