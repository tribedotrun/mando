import type { QueryClient } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';

export function invalidateTaskDetail(client: QueryClient, id?: number): void {
  if (id != null) {
    void client.invalidateQueries({ queryKey: queryKeys.tasks.timeline(id) });
    void client.invalidateQueries({ queryKey: queryKeys.tasks.pr(id) });
    void client.invalidateQueries({ queryKey: queryKeys.tasks.askHistory(id) });
    return;
  }

  // Invalidate all task sub-queries (timeline, pr, ask-history for every task)
  void client.invalidateQueries({ queryKey: queryKeys.tasks.all });
}
