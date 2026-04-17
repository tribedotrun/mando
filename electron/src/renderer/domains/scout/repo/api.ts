import type {
  ScoutResponse,
  ScoutItem,
  AskResponse,
  ScoutArticleResponse,
  ActResponse,
  ScoutItemSession,
} from '#renderer/global/types';
import { apiGet, apiPost, apiPatch, buildUrl } from '#renderer/global/providers/http';

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
export async function askScout(
  id: number,
  question: string,
  sessionId?: string,
  images?: File[],
): Promise<AskResponse> {
  if (images?.length) {
    const form = new FormData();
    form.append('id', String(id));
    form.append('question', question);
    if (sessionId) form.append('session_id', sessionId);
    for (const img of images) form.append('images', img, img.name);
    const res = await fetch(buildUrl('/api/scout/ask'), { method: 'POST', body: form });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: res.statusText }));
      throw new Error(err.error || `HTTP ${res.status}`);
    }
    return res.json() as Promise<AskResponse>;
  }
  return apiPost<AskResponse>('/api/scout/ask', { id, question, session_id: sessionId });
}
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
export const fetchResearchRuns = () =>
  apiGet<import('#renderer/global/types').ScoutResearchRun[]>('/api/scout/research');
export const fetchResearchRunItems = (runId: number) =>
  apiGet<import('#renderer/global/types').ScoutItem[]>(`/api/scout/research/${runId}/items`);
