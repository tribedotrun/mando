import { useQuery } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import {
  fetchTasks,
  fetchAskHistory,
  fetchFeed,
  fetchArtifacts,
  fetchWorkers,
  fetchActivityStats,
  fetchTimeline,
  fetchItemSessions,
  fetchPrSummary,
} from '#renderer/domains/captain/repo/api';
import {
  listTerminals,
  fetchWorkbenches,
  type TerminalSessionInfo,
  type WorkbenchItem,
  type WorkbenchStatusFilter,
} from '#renderer/domains/captain/repo/terminal-api';
import log from '#renderer/global/service/logger';
import type {
  TaskListResponse,
  AskHistoryResponse,
  FeedResponse,
  ArtifactsResponse,
  WorkersResponse,
  ActivityStatsResponse,
  SessionSummary,
} from '#renderer/global/types';

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
// Terminals & Workbenches
// ---------------------------------------------------------------------------

export function useTerminalList() {
  return useQuery<TerminalSessionInfo[]>({
    queryKey: queryKeys.terminals.list(),
    queryFn: () => listTerminals(),
  });
}

export function useWorkbenchList(status?: WorkbenchStatusFilter) {
  return useQuery<WorkbenchItem[]>({
    queryKey: queryKeys.workbenches.list(status),
    queryFn: () => fetchWorkbenches(status),
  });
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

export function useActivityStats() {
  return useQuery<ActivityStatsResponse>({
    queryKey: queryKeys.stats.activity(),
    queryFn: () => fetchActivityStats(),
    staleTime: 5 * 60 * 1000,
  });
}

// ---------------------------------------------------------------------------
// Timeline + Sessions (combined query for task detail)
// ---------------------------------------------------------------------------

export function useTaskTimelineData(id: number) {
  return useQuery({
    queryKey: queryKeys.tasks.timeline(id),
    queryFn: async () => {
      const [tl, sess] = await Promise.all([fetchTimeline(id), fetchItemSessions(id)]);
      const map: Record<string, SessionSummary> = {};
      for (const s of sess.sessions) map[s.session_id] = s;
      return { events: tl.events, sessionMap: map, sessions: sess.sessions };
    },
    enabled: id > 0,
  });
}

// ---------------------------------------------------------------------------
// PR Summary
// ---------------------------------------------------------------------------

export function useTaskPrSummary(id: number, prNumber: number | undefined, isFinalized: boolean) {
  return useQuery({
    queryKey: queryKeys.tasks.pr(id),
    queryFn: async () => {
      const data = await fetchPrSummary(id);
      if (isFinalized && data.summary) {
        localStorage.setItem(`pr-cache:${id}`, JSON.stringify(data));
      }
      return data;
    },
    enabled: !!prNumber,
    staleTime: isFinalized ? Infinity : 30_000,
    initialData: () => {
      if (!isFinalized) return undefined;
      const key = `pr-cache:${id}`;
      const cached = localStorage.getItem(key);
      if (!cached) return undefined;
      try {
        return JSON.parse(cached);
      } catch (err) {
        log.warn(`[TaskDetail] corrupted pr-cache for item ${id}, clearing:`, err);
        localStorage.removeItem(key);
        return undefined;
      }
    },
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
