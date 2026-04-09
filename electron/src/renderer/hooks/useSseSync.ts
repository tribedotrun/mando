/**
 * SSE-to-cache bridge: connects to the daemon's SSE stream and keeps the
 * React Query cache in sync.
 *
 * Two sync tiers:
 *   Tier 1 (tasks, scout, workbenches, terminals): SSE carries the changed
 *     item — patched directly into the query cache via setQueryData,
 *     rev-guarded to reject out-of-order events.
 *   Tier 2 (status, sessions): SSE carries affected entity IDs — triggers
 *     targeted invalidateQueries for the relevant detail caches.
 *
 * On initial connect, the daemon sends a snapshot that seeds all list caches.
 * On reconnect after disconnect, a single full invalidateQueries catches up.
 */

import { useCallback, useRef } from 'react';
import { useQueryClient, type QueryClient } from '@tanstack/react-query';
import { connectSSE, initBaseUrl } from '#renderer/api';
import { queryKeys } from '#renderer/queryKeys';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import type {
  TaskItem,
  ScoutItem,
  SSEConnectionStatus,
  SSEEvent,
  SseEntityPayload,
  SseStatusPayload,
  SseSessionsPayload,
  TaskListResponse,
  WorkersResponse,
  MandoConfig,
} from '#renderer/types';
import type { WorkbenchItem, TerminalSessionInfo } from '#renderer/api-terminal';
import { parseNotification } from '#renderer/global/hooks/useDesktopNotifications';
import { toast } from 'sonner';
import log from '#renderer/logger';

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

function patchTaskList(qc: QueryClient, payload: SseEntityPayload<TaskItem>): void {
  patchListItem<TaskItem>(
    qc,
    queryKeys.tasks.list(),
    payload,
    (data) => (data as TaskListResponse | undefined)?.items,
    (data, items) => ({ ...(data as TaskListResponse), items, count: items.length }),
  );

  // Also invalidate detail caches for this specific task
  const id = payload.item?.id ?? (payload.id as number | undefined);
  if (id != null) {
    void qc.invalidateQueries({ queryKey: queryKeys.tasks.timeline(id) });
    void qc.invalidateQueries({ queryKey: queryKeys.tasks.pr(id) });
    void qc.invalidateQueries({ queryKey: queryKeys.tasks.askHistory(id) });
  }
}

function patchScoutList(qc: QueryClient, payload: SseEntityPayload<ScoutItem>): void {
  // Scout list is paginated — invalidate all scout list queries for simplicity
  // since the paginated key includes page/status and we can't know which page changed.
  if (payload.action === 'created' || payload.action === 'deleted') {
    void qc.invalidateQueries({ queryKey: queryKeys.scout.all });
    return;
  }
  // For updates, try to patch any cached scout list pages
  void qc.invalidateQueries({ queryKey: queryKeys.scout.all });
}

function patchWorkbenchList(qc: QueryClient, payload: SseEntityPayload<WorkbenchItem>): void {
  patchListItem<WorkbenchItem>(
    qc,
    queryKeys.workbenches.list(),
    payload,
    (data) => data as WorkbenchItem[] | undefined,
    (_data, items) => items,
  );
}

function patchTerminalList(qc: QueryClient, payload: SseEntityPayload<TerminalSessionInfo>): void {
  patchListItem<TerminalSessionInfo>(
    qc,
    queryKeys.terminals.list(),
    payload,
    (data) => data as TerminalSessionInfo[] | undefined,
    (_data, items) => items,
  );
}

// ── Tier 2: aggregate/detail invalidation ──

function handleStatusEvent(qc: QueryClient, data: SseStatusPayload | null): void {
  void qc.invalidateQueries({ queryKey: queryKeys.workers.list() });
  if (data?.affected_task_ids) {
    for (const id of data.affected_task_ids) {
      void qc.invalidateQueries({ queryKey: queryKeys.tasks.timeline(id) });
      void qc.invalidateQueries({ queryKey: queryKeys.tasks.pr(id) });
    }
  }
}

function handleSessionsEvent(qc: QueryClient, data: SseSessionsPayload | null): void {
  void qc.invalidateQueries({ queryKey: queryKeys.sessions.all });
  if (data?.affected_task_ids) {
    for (const id of data.affected_task_ids) {
      void qc.invalidateQueries({ queryKey: queryKeys.tasks.timeline(id) });
    }
  }
}

// ── Snapshot seeding ──

function seedFromSnapshot(qc: QueryClient, snapshot: SSEEvent): void {
  const d = snapshot.data as Record<string, unknown> | undefined;
  if (!d) return;

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

  // Scout items from snapshot (if included)
  if (d.scout_items && Array.isArray(d.scout_items)) {
    // Don't seed paginated scout list from snapshot — it would need
    // pagination metadata we don't have. Let the first query fetch naturally.
  }

  log.debug('[sse] snapshot seeded caches', {
    tasks: Array.isArray(d.tasks) ? (d.tasks as unknown[]).length : 0,
    workers: Array.isArray(d.workers) ? (d.workers as unknown[]).length : 0,
  });
}

// ── Main hook ──

export interface UseSseSyncOptions {
  onStatusChange?: (status: SSEConnectionStatus) => void;
  /** Called before SSE connects to check if onboarding is needed */
  onBootstrap?: () => Promise<boolean>;
  /** Desktop notification processor from useDesktopNotifications */
  processDesktopNotification?: (event: SSEEvent) => void;
  /** Called when init fails (e.g. daemon unreachable) */
  onError?: (message: string) => void;
}

export function useSseSync(options?: UseSseSyncOptions): SSEConnectionStatus {
  const qc = useQueryClient();
  const sseRef = useRef<EventSource | null>(null);
  const statusRef = useRef<SSEConnectionStatus>('connecting');
  const prevStatusRef = useRef<SSEConnectionStatus>('connected');
  const setStatus = useCallback(
    (s: SSEConnectionStatus) => {
      statusRef.current = s;
      options?.onStatusChange?.(s);
    },
    [options],
  );

  // Store options in a ref so useMountEffect captures the latest values
  const optionsRef = useRef(options);
  optionsRef.current = options;

  useMountEffect(() => {
    let cancelled = false;

    async function start() {
      try {
        await initBaseUrl();

        // Bootstrap check (onboarding detection)
        if (optionsRef.current?.onBootstrap) {
          const needsOnboarding = await optionsRef.current.onBootstrap();
          if (needsOnboarding || cancelled) return;
        }

        if (cancelled) return;

        const source = connectSSE(
          // onEvent
          (event: SSEEvent) => {
            switch (event.event) {
              case 'snapshot':
                seedFromSnapshot(qc, event);
                break;

              case 'tasks':
                if (
                  event.data &&
                  typeof event.data === 'object' &&
                  'item' in (event.data as Record<string, unknown>)
                ) {
                  patchTaskList(qc, event.data as SseEntityPayload<TaskItem>);
                } else {
                  // Legacy: empty signal, fall back to invalidation
                  void qc.invalidateQueries({ queryKey: queryKeys.tasks.list() });
                  void qc.invalidateQueries({ queryKey: queryKeys.workers.list() });
                }
                break;

              case 'scout':
                if (
                  event.data &&
                  typeof event.data === 'object' &&
                  'item' in (event.data as Record<string, unknown>)
                ) {
                  patchScoutList(qc, event.data as SseEntityPayload<ScoutItem>);
                } else {
                  void qc.invalidateQueries({ queryKey: queryKeys.scout.all });
                }
                break;

              case 'workbenches':
                if (
                  event.data &&
                  typeof event.data === 'object' &&
                  'item' in (event.data as Record<string, unknown>)
                ) {
                  patchWorkbenchList(qc, event.data as SseEntityPayload<WorkbenchItem>);
                } else {
                  void qc.invalidateQueries({ queryKey: queryKeys.workbenches.all });
                }
                break;

              case 'terminals':
                if (
                  event.data &&
                  typeof event.data === 'object' &&
                  'item' in (event.data as Record<string, unknown>)
                ) {
                  patchTerminalList(qc, event.data as SseEntityPayload<TerminalSessionInfo>);
                } else {
                  void qc.invalidateQueries({ queryKey: queryKeys.terminals.all });
                }
                break;

              case 'status':
                handleStatusEvent(qc, (event.data as SseStatusPayload) ?? null);
                break;

              case 'sessions':
                handleSessionsEvent(qc, (event.data as SseSessionsPayload) ?? null);
                break;

              case 'notification': {
                const payload = parseNotification(event);
                if (payload) {
                  if (payload.kind?.type === 'RateLimited') {
                    const fn = payload.kind.status === 'rejected' ? toast.error : toast.info;
                    fn(payload.message);
                  }
                } else if (event.data) {
                  log.warn('[sse] unexpected notification shape:', event.data);
                }
                break;
              }

              case 'config':
                void qc.invalidateQueries({ queryKey: queryKeys.config.all });
                break;

              case 'resync':
                log.warn('[sse] resync — invalidating all caches');
                void qc.invalidateQueries();
                break;

              default:
                break;
            }

            // Desktop notifications for all events
            optionsRef.current?.processDesktopNotification?.(event);
          },

          // onStatusChange
          (newStatus: SSEConnectionStatus) => {
            const wasDisconnected = prevStatusRef.current === 'disconnected';
            prevStatusRef.current = newStatus;
            setStatus(newStatus);

            if (newStatus === 'connected' && wasDisconnected) {
              // Reconnect: full invalidation to catch up on missed events
              log.info('[sse] reconnected — invalidating all caches');
              void qc.invalidateQueries();
            }
          },
        );

        sseRef.current = source;
      } catch (err) {
        log.error('[sse] init failed:', err);
        const msg = err instanceof Error ? err.message : 'Unknown daemon connection error';
        optionsRef.current?.onError?.(msg);
      }
    }

    void start();

    return () => {
      cancelled = true;
      sseRef.current?.close();
      sseRef.current = null;
    };
  });

  return statusRef.current;
}
