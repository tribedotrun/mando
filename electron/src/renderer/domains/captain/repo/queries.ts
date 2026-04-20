import { useQuery } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { defineJsonKeyspace } from '#renderer/global/providers/persistence';
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
import { prSummaryResponseSchema } from '#shared/daemon-contract/schemas';
import { toReactQuery } from '#result';
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
    queryFn: () => toReactQuery(fetchTasks()),
  });
}

/** Fetch all tasks including those with archived workbenches. Separate key
 *  so SSE patches to the canonical list key are not affected. Only enabled
 *  when `showArchived` is true. */
export function useTaskListWithArchived(enabled: boolean) {
  return useQuery<TaskListResponse>({
    queryKey: [...queryKeys.tasks.list(), 'with-archived'],
    queryFn: () => toReactQuery(fetchTasks(true)),
    enabled,
  });
}

export function useTaskAskHistory(id: number) {
  return useQuery<AskHistoryResponse>({
    queryKey: queryKeys.tasks.askHistory(id),
    queryFn: () => toReactQuery(fetchAskHistory(id)),
    enabled: id > 0,
  });
}

export function useTaskFeed(id: number) {
  return useQuery<FeedResponse>({
    queryKey: queryKeys.tasks.feed(id),
    queryFn: () => toReactQuery(fetchFeed(id)),
    enabled: id > 0,
  });
}

export function useTaskArtifacts(id: number) {
  return useQuery<ArtifactsResponse>({
    queryKey: queryKeys.tasks.artifacts(id),
    queryFn: () => toReactQuery(fetchArtifacts(id)),
    enabled: id > 0,
  });
}

// ---------------------------------------------------------------------------
// Terminals & Workbenches
// ---------------------------------------------------------------------------

export function useTerminalList() {
  return useQuery<TerminalSessionInfo[]>({
    queryKey: queryKeys.terminals.list(),
    queryFn: () => toReactQuery(listTerminals()),
  });
}

export function useWorkbenchList(status?: WorkbenchStatusFilter) {
  return useQuery<WorkbenchItem[]>({
    queryKey: queryKeys.workbenches.list(status),
    queryFn: () => toReactQuery(fetchWorkbenches(status)),
  });
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

export function useActivityStats() {
  return useQuery<ActivityStatsResponse>({
    queryKey: queryKeys.stats.activity(),
    queryFn: () => toReactQuery(fetchActivityStats()),
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
      const [tl, sess] = await Promise.all([
        toReactQuery(fetchTimeline(id)),
        toReactQuery(fetchItemSessions(id)),
      ]);
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

const prCacheStore = defineJsonKeyspace(
  'pr-cache:',
  prSummaryResponseSchema,
  'domains/captain/repo/queries#useTaskPrSummary',
);

export function useTaskPrSummary(id: number, prNumber: number | undefined, isFinalized: boolean) {
  return useQuery({
    queryKey: queryKeys.tasks.pr(id),
    queryFn: async () => {
      const data = await toReactQuery(fetchPrSummary(id));
      if (isFinalized && data.summary) {
        prCacheStore.for(String(id)).write(data);
      }
      return data;
    },
    enabled: !!prNumber,
    staleTime: isFinalized ? Infinity : 30_000,
    initialData: () => {
      if (!isFinalized) return undefined;
      const slot = prCacheStore.for(String(id));
      const cached = slot.read();
      if (cached === undefined) {
        // Cache miss or schema-invalid (defineJsonSlot already cleared and logged).
        return undefined;
      }
      log.debug(`[TaskDetail] hydrated pr-cache for item ${id}`);
      return cached;
    },
  });
}

// ---------------------------------------------------------------------------
// Workers
// ---------------------------------------------------------------------------

export function useWorkers() {
  return useQuery<WorkersResponse>({
    queryKey: queryKeys.workers.list(),
    queryFn: () => toReactQuery(fetchWorkers()),
    refetchInterval: 15_000,
  });
}
