import type { SessionsResponse, TranscriptResponse } from '#renderer/types';
import { apiGet } from '#renderer/api';

export async function fetchSessions(page = 1, perPage = 50, category?: string, status?: string) {
  const params = new URLSearchParams({ page: String(page), per_page: String(perPage) });
  if (category) params.set('category', category);
  if (status && status !== 'all') params.set('status', status);
  return apiGet<SessionsResponse>(`/api/sessions?${params}`);
}

export const fetchTranscript = (sessionId: string) =>
  apiGet<TranscriptResponse>(`/api/sessions/${sessionId}/transcript`);
