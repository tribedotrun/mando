import { useQuery } from '@tanstack/react-query';
import { fetchTranscript } from '#renderer/domains/sessions/repo/api';
import { queryKeys } from '#renderer/global/repo/queryKeys';

export function useTranscript(sessionId: string | null) {
  return useQuery({
    queryKey: sessionId ? queryKeys.sessions.transcript(sessionId) : ['noop'],
    queryFn: () => fetchTranscript(sessionId!),
    enabled: !!sessionId,
  });
}
