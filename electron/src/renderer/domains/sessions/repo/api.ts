import { apiGetRouteR } from '#renderer/global/providers/http';
import type { SessionCategory, SessionStatus } from '#renderer/global/types';

export function fetchSessions(
  page = 1,
  perPage = 50,
  category?: SessionCategory,
  status?: SessionStatus,
) {
  return apiGetRouteR('getSessions', {
    query: {
      page,
      per_page: perPage,
      category,
      status,
    },
  });
}

export const fetchSessionJsonlPath = (sessionId: string) =>
  apiGetRouteR('getSessionsByIdJsonlpath', { params: { id: sessionId } });
