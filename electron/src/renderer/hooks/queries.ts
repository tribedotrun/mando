import { useMemo } from 'react';
import { useQuery, keepPreviousData } from '@tanstack/react-query';
import { queryKeys } from '#renderer/queryKeys';
import {
  fetchTasks,
  fetchAskHistory,
  fetchFeed,
  fetchArtifacts,
  fetchWorkers,
  fetchActivityStats,
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
  ActivityStatsResponse,
  DailyMerge,
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
// Stats
// ---------------------------------------------------------------------------

export function useActivityStats() {
  return useQuery<ActivityStatsResponse>({
    queryKey: queryKeys.stats.activity(),
    queryFn: () => fetchActivityStats(),
    staleTime: 5 * 60 * 1000,
  });
}

const ACTIVITY_STRIP_DAYS = 56;

function buildCountMap(merges: DailyMerge[]): Map<string, number> {
  const map = new Map<string, number>();
  for (const m of merges) map.set(m.date, m.count);
  return map;
}

function lastNDays(n: number): string[] {
  const dates: string[] = [];
  const now = new Date();
  for (let i = n - 1; i >= 0; i--) {
    const d = new Date(now);
    d.setDate(d.getDate() - i);
    dates.push(d.toISOString().slice(0, 10));
  }
  return dates;
}

function computeThresholds(counts: number[]): [number, number, number] {
  const nonzero = counts.filter((c) => c > 0);
  if (nonzero.length === 0) return [1, 2, 3];
  const max = Math.max(...nonzero);
  if (max <= 3) return [1, 2, 3];
  const third = max / 3;
  return [Math.ceil(third), Math.ceil(third * 2), max];
}

/** Group dates into week columns (Mon-Sun), returning a 7-row x N-col grid. */
function buildGrid(dates: string[]): (string | null)[][] {
  const rows: (string | null)[][] = Array.from({ length: 7 }, () => []);
  let col = 0;
  for (let i = 0; i < dates.length; i++) {
    const d = new Date(dates[i] + 'T00:00:00');
    const dow = (d.getDay() + 6) % 7; // Mon=0 .. Sun=6
    if (i > 0 && dow === 0) col++;
    while (rows[dow].length < col) rows[dow].push(null);
    rows[dow].push(dates[i]);
  }
  const maxCols = Math.max(...rows.map((r) => r.length));
  for (const row of rows) {
    while (row.length < maxCols) row.push(null);
  }
  return rows;
}

export interface ActivityStripData {
  grid: (string | null)[][];
  countMap: Map<string, number>;
  thresholds: [number, number, number];
  hasMerges: boolean;
}

export function useActivityStripData(): ActivityStripData {
  const { data } = useActivityStats();
  return useMemo(() => {
    const merges = data?.daily_merges ?? [];
    const map = buildCountMap(merges);
    const days = lastNDays(ACTIVITY_STRIP_DAYS);
    const counts = days.map((d) => map.get(d) ?? 0);
    return {
      grid: buildGrid(days),
      countMap: map,
      thresholds: computeThresholds(counts),
      hasMerges: counts.some((c) => c > 0),
    };
  }, [data?.daily_merges]);
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
