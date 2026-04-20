import { apiGetRouteR } from '#renderer/global/providers/http';

export function fetchSessions(page = 1, perPage = 50, category?: string, status?: string) {
  return apiGetRouteR('getSessions', {
    query: {
      page,
      per_page: perPage,
      category,
      status: status && status !== 'all' ? status : undefined,
    },
  });
}

export const fetchTranscript = (sessionId: string) =>
  apiGetRouteR('getSessionsByIdTranscript', { params: { id: sessionId } });
