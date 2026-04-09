import { useQuery, keepPreviousData } from '@tanstack/react-query';
import { queryKeys } from '#renderer/queryKeys';
import {
  fetchTasks,
  fetchTimeline,
  fetchItemSessions,
  fetchPrSummary,
  fetchAskHistory,
  fetchWorkers,
  fetchSessions,
  apiGet,
} from '#renderer/api';
import {
  fetchScoutItems,
  fetchScoutItem,
  fetchScoutArticle,
  fetchScoutItemSessions,
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
  TimelineResponse,
  ItemSessionsResponse,
  PrSummaryResponse,
  AskHistoryResponse,
  ScoutResponse,
  ScoutItem,
  ScoutArticleResponse,
  ScoutItemSession,
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

export function useTaskTimeline(id: number) {
  return useQuery<[TimelineResponse, ItemSessionsResponse]>({
    queryKey: queryKeys.tasks.timeline(id),
    queryFn: () => Promise.all([fetchTimeline(id), fetchItemSessions(id)]),
    enabled: id > 0,
  });
}

export function useTaskPrSummary(id: number, prNumber: number | undefined, isFinalized: boolean) {
  return useQuery<PrSummaryResponse>({
    queryKey: queryKeys.tasks.pr(id),
    queryFn: async () => {
      const result = await fetchPrSummary(id);
      if (isFinalized) {
        try {
          localStorage.setItem(`pr-cache:${id}`, JSON.stringify(result));
        } catch {
          // localStorage full or unavailable -- ignore
        }
      }
      return result;
    },
    enabled: !!prNumber,
    staleTime: isFinalized ? Infinity : 30_000,
    initialData: () => {
      if (!isFinalized) return undefined;
      try {
        const cached = localStorage.getItem(`pr-cache:${id}`);
        if (cached) return JSON.parse(cached) as PrSummaryResponse;
      } catch {
        // corrupt cache -- ignore
      }
      return undefined;
    },
  });
}

export function useTaskAskHistory(id: number) {
  return useQuery<AskHistoryResponse>({
    queryKey: queryKeys.tasks.askHistory(id),
    queryFn: () => fetchAskHistory(id),
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

export function useScoutItem(id: number) {
  return useQuery<ScoutItem>({
    queryKey: queryKeys.scout.item(id),
    queryFn: () => fetchScoutItem(id),
    enabled: id > 0,
  });
}

export function useScoutArticle(id: number) {
  return useQuery<ScoutArticleResponse>({
    queryKey: queryKeys.scout.article(id),
    queryFn: () => fetchScoutArticle(id),
    enabled: id > 0,
  });
}

export function useScoutItemSessions(id: number, enabled?: boolean) {
  return useQuery<ScoutItemSession[]>({
    queryKey: queryKeys.scout.sessions(id),
    queryFn: () => fetchScoutItemSessions(id),
    enabled: enabled !== false && id > 0,
  });
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

export function useSessionsList(page: number, category?: string) {
  return useQuery<SessionsResponse>({
    queryKey: queryKeys.sessions.list(page, category),
    queryFn: () => fetchSessions(page, 50, category),
    placeholderData: keepPreviousData,
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

export function useTelegramHealth() {
  return useQuery({
    queryKey: queryKeys.health.telegram(),
    queryFn: () => apiGet<{ ok: boolean }>('/api/health/telegram'),
    refetchInterval: 10_000,
  });
}
