import { useQuery, keepPreviousData } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { fetchSessions } from '#renderer/domains/sessions/repo/api';
import type { SessionsResponse } from '#renderer/global/types';

export function useSessionsList(page: number, category?: string, status?: string) {
  return useQuery<SessionsResponse>({
    queryKey: queryKeys.sessions.list(page, category, status),
    queryFn: () => fetchSessions(page, 50, category, status),
    // Only retain stale data while paginating (page > 1); clear immediately on
    // filter changes (page resets to 1) so stale mismatched rows are not shown.
    placeholderData: page > 1 ? keepPreviousData : undefined,
  });
}
