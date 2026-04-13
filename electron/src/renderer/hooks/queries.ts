import { useQuery, keepPreviousData } from '@tanstack/react-query';
import { queryKeys } from '#renderer/queryKeys';
import {
  fetchTasks,
  fetchAskHistory,
  fetchFeed,
  fetchArtifacts,
  fetchWorkers,
  apiGet,
} from '#renderer/api';
import { fetchSessions } from '#renderer/api-sessions';
import {
  fetchScoutItems,
  fetchResearchRuns,
  fetchResearchRunItems,
  type ScoutQueryParams,
} from '#renderer/api-scout';

export type { ScoutQueryParams };
import {
  listTerminals,
  fetchWorkbenches,
  type TerminalSessionInfo,
  type WorkbenchItem,
} from '#renderer/api-terminal';
import type {
  TaskListResponse,
  AskHistoryResponse,
  FeedResponse,
  ArtifactsResponse,
  ScoutResponse,
  ScoutItem,
  ScoutResearchRun,
  WorkersResponse,
  SessionsResponse,
  MandoConfig,
} from '#renderer/types';

export type { TerminalSessionInfo };

// ---------------------------------------------------------------------------
// Tasks
// ---------------------------------------------------------------------------

export function useTaskList() {
  return useQuery<TaskListResponse>({
    queryKey: queryKeys.tasks.list(),
    queryFn: () => fetchTasks(),
  });
}

/** Fetch all tasks including those with archived workbenches. Separate key
 *  so SSE patches to the canonical list key are not affected. Only enabled
 *  when `showArchived` is true. */
export function useTaskListWithArchived(enabled: boolean) {
  return useQuery<TaskListResponse>({
    queryKey: [...queryKeys.tasks.list(), 'with-archived'],
    queryFn: () => fetchTasks(true),
    enabled,
  });
}

export function useTaskAskHistory(id: number) {
  return useQuery<AskHistoryResponse>({
    queryKey: queryKeys.tasks.askHistory(id),
    queryFn: () => fetchAskHistory(id),
    enabled: id > 0,
  });
}

export function useTaskFeed(id: number) {
  return useQuery<FeedResponse>({
    queryKey: queryKeys.tasks.feed(id),
    queryFn: () => fetchFeed(id),
    enabled: id > 0,
  });
}

export function useTaskArtifacts(id: number) {
  return useQuery<ArtifactsResponse>({
    queryKey: queryKeys.tasks.artifacts(id),
    queryFn: () => fetchArtifacts(id),
    enabled: id > 0,
  });
}

// ---------------------------------------------------------------------------
// Scout
// ---------------------------------------------------------------------------

export function useScoutList(params?: ScoutQueryParams) {
  return useQuery<ScoutResponse>({
    queryKey: queryKeys.scout.list({
      status: params?.status,
      page: params?.page,
      q: params?.q,
      type: params?.type,
    }),
    queryFn: () => fetchScoutItems(params),
    placeholderData: keepPreviousData,
  });
}

export function useResearchRuns() {
  return useQuery<ScoutResearchRun[]>({
    queryKey: queryKeys.scout.research(),
    queryFn: fetchResearchRuns,
  });
}

export function useResearchRunItems(runId: number) {
  return useQuery<ScoutItem[]>({
    queryKey: queryKeys.scout.researchItems(runId),
    queryFn: () => fetchResearchRunItems(runId),
    enabled: runId > 0,
  });
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

export function useSessionsList(page: number, category?: string, status?: string) {
  return useQuery<SessionsResponse>({
    queryKey: queryKeys.sessions.list(page, category, status),
    queryFn: () => fetchSessions(page, 50, category, status),
    // Only retain stale data while paginating (page > 1); clear immediately on
    // filter changes (page resets to 1) so stale mismatched rows are not shown.
    placeholderData: page > 1 ? keepPreviousData : undefined,
  });
}

// ---------------------------------------------------------------------------
// Terminals & Workbenches
// ---------------------------------------------------------------------------

export function useTerminalList() {
  return useQuery<TerminalSessionInfo[]>({
    queryKey: queryKeys.terminals.list(),
    queryFn: () => listTerminals(),
  });
}

export function useWorkbenchList() {
  return useQuery<WorkbenchItem[]>({
    queryKey: queryKeys.workbenches.list(),
    queryFn: () => fetchWorkbenches(),
  });
}

// ---------------------------------------------------------------------------
// Workers
// ---------------------------------------------------------------------------

export function useWorkers() {
  return useQuery<WorkersResponse>({
    queryKey: queryKeys.workers.list(),
    queryFn: () => fetchWorkers(),
    refetchInterval: 15_000,
  });
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

export function useConfig() {
  return useQuery<MandoConfig>({
    queryKey: queryKeys.config.current(),
    queryFn: async (): Promise<MandoConfig> => {
      try {
        return await apiGet<MandoConfig>('/api/config');
      } catch {
        const raw = await window.mandoAPI.readConfig();
        return (typeof raw === 'string' ? JSON.parse(raw) : raw) as MandoConfig;
      }
    },
  });
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------
