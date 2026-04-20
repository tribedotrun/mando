import { useQuery, keepPreviousData } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { fetchSessions, fetchTranscript } from '#renderer/domains/sessions/repo/api';
import type { SessionsResponse } from '#renderer/global/types';
import { toReactQuery } from '#result';

export function useSessionsList(page: number, category?: string, status?: string) {
  return useQuery<SessionsResponse>({
    queryKey: queryKeys.sessions.list(page, category, status),
    queryFn: () => toReactQuery(fetchSessions(page, 50, category, status)),
    // Only retain stale data while paginating (page > 1); clear immediately on
    // filter changes (page resets to 1) so stale mismatched rows are not shown.
    placeholderData: page > 1 ? keepPreviousData : undefined,
  });
}

export function useTranscript(sessionId: string | null) {
  const keySessionId = sessionId ?? '__disabled__';
  return useQuery({
    queryKey: queryKeys.sessions.transcript(keySessionId),
    queryFn: () => toReactQuery(fetchTranscript(sessionId!)),
    enabled: !!sessionId,
  });
}
