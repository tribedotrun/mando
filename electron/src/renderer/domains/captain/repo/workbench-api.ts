import { apiGetRouteR, apiPatchRouteR } from '#renderer/global/providers/http';
import type { WorkbenchItem, WorkbenchStatusFilter } from '#renderer/global/types';
import { type ApiError, type ResultAsync } from '#result';

export type { WorkbenchItem, WorkbenchStatusFilter } from '#renderer/global/types';

export function fetchWorkbenches(
  status?: WorkbenchStatusFilter,
): ResultAsync<WorkbenchItem[], ApiError> {
  return apiGetRouteR('getWorkbenches', {
    query: status && status !== 'active' ? { status } : undefined,
  }).map((res) => res.workbenches);
}

export function archiveWorkbench(id: number): ResultAsync<WorkbenchItem, ApiError> {
  return apiPatchRouteR('patchWorkbenchesById', { archived: true }, { params: { id } });
}

export function unarchiveWorkbench(id: number): ResultAsync<WorkbenchItem, ApiError> {
  return apiPatchRouteR('patchWorkbenchesById', { archived: false }, { params: { id } });
}

export function pinWorkbench(id: number, pinned: boolean): ResultAsync<WorkbenchItem, ApiError> {
  return apiPatchRouteR('patchWorkbenchesById', { pinned }, { params: { id } });
}

export function renameWorkbench(id: number, title: string): ResultAsync<WorkbenchItem, ApiError> {
  return apiPatchRouteR('patchWorkbenchesById', { title }, { params: { id } });
}
