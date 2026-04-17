/**
 * SSE-to-cache bridge helpers: pure QueryClient manipulation functions
 * used by useSseSync to keep the React Query cache in sync with SSE events.
 *
 * Two sync tiers:
 *   Tier 1 (tasks, scout, workbenches, terminals): SSE carries the changed
 *     item -- patched directly into the query cache via setQueryData,
 *     rev-guarded to reject out-of-order events.
 *   Tier 2 (status, sessions): SSE carries affected entity IDs -- triggers
 *     targeted invalidateQueries for the relevant detail caches.
 */

import type { QueryClient } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import type {
  TaskItem,
  ScoutItem,
  SSEEvent,
  SseEntityPayload,
  SseStatusPayload,
  SseSessionsPayload,
  TaskListResponse,
  WorkersResponse,
  MandoConfig,
  WorkbenchItem,
  TerminalSessionInfo,
} from '#renderer/global/types';

// ── Tier 1: rev-guarded entity list patching ──

function patchListItem<T extends { id: number | string; rev: number }>(
  qc: QueryClient,
  queryKey: readonly unknown[],
  payload: SseEntityPayload<T>,
  getList: (data: unknown) => T[] | undefined,
  setList: (data: unknown, items: T[]) => unknown,
): void {
  if (payload.action === 'created' && payload.item) {
    qc.setQueryData(queryKey, (old: unknown) => {
      const list = getList(old);
      if (!list) return old;
      // Avoid duplicates
      if (list.some((i) => i.id === payload.item!.id)) {
        return setList(
          old,
          list.map((i) => (i.id === payload.item!.id ? payload.item! : i)),
        );
      }
      return setList(old, [payload.item!, ...list]);
    });
  } else if (payload.action === 'updated' && payload.item) {
    qc.setQueryData(queryKey, (old: unknown) => {
      const list = getList(old);
      if (!list) return old;
      return setList(
        old,
        list.map((existing) => {
          if (existing.id !== payload.item!.id) return existing;
          // Rev guard: reject stale events
          if (payload.item!.rev <= existing.rev) return existing;
          return payload.item!;
        }),
      );
    });
  } else if (payload.action === 'deleted') {
    const deleteId = payload.id ?? payload.item?.id;
    if (deleteId == null) return;
    qc.setQueryData(queryKey, (old: unknown) => {
      const list = getList(old);
      if (!list) return old;
      return setList(
        old,
        list.filter((i) => i.id !== deleteId),
      );
    });
  }
}

// ── Task list helpers (TaskListResponse wraps items + count) ──

export function patchTaskList(qc: QueryClient, payload: SseEntityPayload<TaskItem>): void {
  patchListItem<TaskItem>(
    qc,
    queryKeys.tasks.list(),
    payload,
    (data) => (data as TaskListResponse | undefined)?.items,
    (data, items) => ({ ...(data as TaskListResponse), items, count: items.length }),
  );

  // Invalidate activity stats only when a task reaches merged (or is deleted)
  if (payload.item?.status === 'merged' || payload.action === 'deleted') {
    void qc.invalidateQueries({ queryKey: queryKeys.stats.all });
  }

  // Also invalidate detail caches for this specific task
  const id = payload.item?.id ?? (payload.id as number | undefined);
  if (id != null) {
    void qc.invalidateQueries({ queryKey: queryKeys.tasks.timeline(id) });
    void qc.invalidateQueries({ queryKey: queryKeys.tasks.askHistory(id) });
    void qc.invalidateQueries({ queryKey: queryKeys.tasks.feed(id) });
    void qc.invalidateQueries({ queryKey: queryKeys.tasks.artifacts(id) });
  }
}

export function patchScoutList(qc: QueryClient, payload: SseEntityPayload<ScoutItem>): void {
  // Scout list is paginated -- invalidate all scout list queries for simplicity since the paginated key includes page/status and we can't know which page changed.
  if (payload.action === 'created' || payload.action === 'deleted') {
    void qc.invalidateQueries({ queryKey: queryKeys.scout.all });
    return;
  }
  // For updates, try to patch any cached scout list pages
  void qc.invalidateQueries({ queryKey: queryKeys.scout.all });
}

export function patchWorkbenchList(
  qc: QueryClient,
  payload: SseEntityPayload<WorkbenchItem>,
): void {
  patchListItem<WorkbenchItem>(
    qc,
    queryKeys.workbenches.list(),
    payload,
    (data) => data as WorkbenchItem[] | undefined,
    (_data, items) => items,
  );
}

export function patchTerminalList(
  qc: QueryClient,
  payload: SseEntityPayload<TerminalSessionInfo>,
): void {
  patchListItem<TerminalSessionInfo>(
    qc,
    queryKeys.terminals.list(),
    payload,
    (data) => data as TerminalSessionInfo[] | undefined,
    (_data, items) => items,
  );
}

// ── Tier 2: aggregate/detail invalidation ──

export function handleStatusEvent(qc: QueryClient, data: SseStatusPayload | null): void {
  void qc.invalidateQueries({ queryKey: queryKeys.workers.list() });
  if (data?.affected_task_ids) {
    for (const id of data.affected_task_ids) {
      void qc.invalidateQueries({ queryKey: queryKeys.tasks.timeline(id) });
    }
  }
}

export function handleSessionsEvent(qc: QueryClient, data: SseSessionsPayload | null): void {
  void qc.invalidateQueries({ queryKey: queryKeys.sessions.all });
  if (data?.affected_task_ids) {
    for (const id of data.affected_task_ids) {
      void qc.invalidateQueries({ queryKey: queryKeys.tasks.timeline(id) });
    }
  }
}

// ── Snapshot seeding ──

export interface SnapshotCounts {
  tasks: number;
  workers: number;
}

export function seedFromSnapshot(qc: QueryClient, snapshot: SSEEvent): SnapshotCounts {
  const d = snapshot.data as Record<string, unknown> | undefined;
  if (!d) return { tasks: 0, workers: 0 };

  // Seed task list
  if (d.tasks && Array.isArray(d.tasks)) {
    qc.setQueryData(queryKeys.tasks.list(), {
      items: d.tasks as TaskItem[],
      count: (d.tasks as TaskItem[]).length,
    });
  }

  // Seed workers
  if (d.workers && Array.isArray(d.workers)) {
    qc.setQueryData(queryKeys.workers.list(), {
      workers: d.workers,
    } as WorkersResponse);
  }

  // Seed workbenches
  if (d.workbenches && Array.isArray(d.workbenches)) {
    qc.setQueryData(queryKeys.workbenches.list(), d.workbenches as WorkbenchItem[]);
  }

  // Seed terminals
  if (d.terminals && Array.isArray(d.terminals)) {
    qc.setQueryData(queryKeys.terminals.list(), d.terminals as TerminalSessionInfo[]);
  }

  // Seed config
  if (d.config) {
    qc.setQueryData(queryKeys.config.current(), d.config as MandoConfig);
  }

  return {
    tasks: Array.isArray(d.tasks) ? (d.tasks as unknown[]).length : 0,
    workers: Array.isArray(d.workers) ? (d.workers as unknown[]).length : 0,
  };
}

// ── Task detail invalidation ──

export function invalidateTaskDetail(client: QueryClient, id?: number): void {
  if (id != null) {
    void client.invalidateQueries({ queryKey: queryKeys.tasks.timeline(id) });
    void client.invalidateQueries({ queryKey: queryKeys.tasks.pr(id) });
    void client.invalidateQueries({ queryKey: queryKeys.tasks.askHistory(id) });
  } else {
    // Invalidate all task sub-queries (timeline, pr, ask-history for every task)
    void client.invalidateQueries({ queryKey: queryKeys.tasks.all });
  }
}
