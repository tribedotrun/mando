import type { AskResponse } from '#renderer/global/types';
import {
  apiGetRouteR,
  apiMultipartRouteR,
  apiPatchRouteR,
  apiPostRouteR,
} from '#renderer/global/providers/http';
import type { ApiError, ResultAsync } from '#result';

export interface ScoutQueryParams {
  status?: string;
  q?: string;
  type?: string;
  page?: number;
  per_page?: number;
}

// All scout repo functions return ResultAsync. Hooks at the queries.ts/mutations.ts
// layer translate to throw via toReactQuery() at the React Query boundary.

export const fetchScoutItems = (params?: ScoutQueryParams) => {
  return apiGetRouteR('getScoutItems', { query: params });
};
export const fetchScoutItem = (id: number) => apiGetRouteR('getScoutItemsById', { params: { id } });
export const fetchScoutArticle = (id: number) =>
  apiGetRouteR('getScoutItemsByIdArticle', { params: { id } });
export const addScoutUrl = (scoutUrl: string, title?: string) =>
  apiPostRouteR('postScoutItems', { url: scoutUrl, title });
export type ScoutCommand = 'mark_pending' | 'mark_processed' | 'save' | 'archive';

export const updateScoutStatus = (id: number, command: ScoutCommand) =>
  apiPatchRouteR('patchScoutItemsById', { command }, { params: { id } });
export const bulkUpdateScout = (ids: number[], command: ScoutCommand) =>
  apiPostRouteR('postScoutBulk', { ids, command });
export const bulkDeleteScout = (ids: number[]) => apiPostRouteR('postScoutBulkdelete', { ids });
export function askScout(
  id: number,
  question: string,
  sessionId?: string,
  images?: File[],
): ResultAsync<AskResponse, ApiError> {
  if (images?.length) {
    const form = new FormData();
    form.append('id', String(id));
    form.append('question', question);
    if (sessionId) form.append('session_id', sessionId);
    for (const img of images) form.append('images', img, img.name);
    return apiMultipartRouteR('postScoutAsk', form, undefined, {
      id,
      question,
      session_id: sessionId,
    });
  }
  return apiMultipartRouteR('postScoutAsk', {
    id,
    question,
    session_id: sessionId,
  });
}
export const actOnScoutItem = (id: number, project: string, prompt?: string) =>
  apiPostRouteR('postScoutItemsByIdAct', { project, prompt }, { params: { id } });
export const researchScout = (topic: string, process = true) =>
  apiPostRouteR('postScoutResearch', { topic, process });
export const publishScoutTelegraph = (id: number) =>
  apiPostRouteR('postScoutItemsByIdTelegraph', undefined, { params: { id } });
export const fetchResearchRuns = () => apiGetRouteR('getScoutResearch');
export const fetchResearchRunItems = (runId: number) =>
  apiGetRouteR('getScoutResearchByIdItems', { params: { id: runId } });
