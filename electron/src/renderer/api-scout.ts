import type {
  ScoutResponse,
  ScoutItem,
  AskResponse,
  ScoutArticleResponse,
  ActResponse,
  ScoutItemSession,
} from '#renderer/types';
import { apiGet, apiPost, apiPatch } from '#renderer/api';

export interface ScoutQueryParams {
  status?: string;
  q?: string;
  type?: string;
  page?: number;
  per_page?: number;
}

export const fetchScoutItems = (params?: ScoutQueryParams) => {
  const qs = new URLSearchParams();
  if (params?.status) qs.set('status', params.status);
  if (params?.q) qs.set('q', params.q);
  if (params?.type) qs.set('type', params.type);
  if (params?.page != null) qs.set('page', String(params.page));
  if (params?.per_page != null) qs.set('per_page', String(params.per_page));
  const query = qs.toString();
  return apiGet<ScoutResponse>(`/api/scout/items${query ? `?${query}` : ''}`);
};
export const fetchScoutItem = (id: number) => apiGet<ScoutItem>(`/api/scout/items/${id}`);
export const fetchScoutArticle = (id: number) =>
  apiGet<ScoutArticleResponse>(`/api/scout/items/${id}/article`);
export const addScoutUrl = (scoutUrl: string, title?: string) =>
  apiPost<ScoutItem>('/api/scout/items', { url: scoutUrl, title });
export const updateScoutStatus = (id: number, status: string) =>
  apiPatch<ScoutItem>(`/api/scout/items/${id}`, { status });
export const bulkUpdateScout = (ids: number[], updates: { status: string }) =>
  apiPost<void>('/api/scout/bulk', { ids, updates });
export const bulkDeleteScout = (ids: number[]) => apiPost<void>('/api/scout/bulk-delete', { ids });
export const askScout = (id: number, question: string, sessionId?: string) =>
  apiPost<AskResponse>('/api/scout/ask', { id, question, session_id: sessionId });
export const actOnScoutItem = (id: number, project: string, prompt?: string) =>
  apiPost<ActResponse>(`/api/scout/items/${id}/act`, { project, prompt });
export const researchScout = (topic: string, process = true) =>
  apiPost<{ run_id: number }>('/api/scout/research', {
    topic,
    process,
  });
export const publishScoutTelegraph = (id: number) =>
  apiPost<{ ok: boolean; url: string }>(`/api/scout/items/${id}/telegraph`);
export const fetchScoutItemSessions = (id: number) =>
  apiGet<ScoutItemSession[]>(`/api/scout/items/${id}/sessions`);
