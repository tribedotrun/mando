/**
 * SSE-to-cache bridge: connects to the daemon's SSE stream and keeps the
 * React Query cache in sync via helpers from sseCacheHelpers.
 */

import { useCallback, useRef } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { connectSSE, initBaseUrl } from '#renderer/global/providers/http';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import {
  patchTaskList,
  patchScoutList,
  patchWorkbenchList,
  patchTerminalList,
  handleStatusEvent,
  handleSessionsEvent,
  seedFromSnapshot,
} from '#renderer/global/repo/sseCacheHelpers';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import type {
  TaskItem,
  ScoutItem,
  SSEConnectionStatus,
  SSEEvent,
  SseEntityPayload,
  SseStatusPayload,
  SseSessionsPayload,
  WorkbenchItem,
  TerminalSessionInfo,
} from '#renderer/global/types';
import { parseNotification } from '#renderer/global/service/notificationHelpers';
import { toast } from 'sonner';
import log from '#renderer/global/service/logger';

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
              case 'snapshot': {
                const counts = seedFromSnapshot(qc, event);
                log.debug('[sse] snapshot seeded caches', counts);
                break;
              }

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
                  void qc.invalidateQueries({ queryKey: queryKeys.stats.all });
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
                  // Invalidate only filtered variants (archived, all) so they refetch, but preserve the active list's Tier 1 patch-only behavior (zero HTTP refetches in normal operation).
                  void qc.invalidateQueries({
                    queryKey: queryKeys.workbenches.all,
                    predicate: (query) => query.queryKey.length > 2,
                  });
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

              case 'research': {
                const rd = event.data as Record<string, unknown> | undefined;
                if (rd) {
                  const action = rd.action as string | undefined;
                  if (action === 'completed') {
                    const added = (rd.added_count as number) ?? 0;
                    toast.success(`Research complete: ${added} link(s) added`);
                    void qc.invalidateQueries({ queryKey: queryKeys.scout.all });
                  } else if (action === 'failed') {
                    const err = (rd.error as string) ?? 'Unknown error';
                    toast.error(`Research failed: ${err}`);
                    void qc.invalidateQueries({ queryKey: queryKeys.scout.research() });
                  } else if (action === 'started') {
                    void qc.invalidateQueries({ queryKey: queryKeys.scout.research() });
                  } else if (action === 'progress') {
                    void qc.invalidateQueries({ queryKey: queryKeys.scout.research() });
                  }
                }
                break;
              }

              case 'artifacts': {
                const ad = event.data as { task_id?: number } | undefined;
                if (ad?.task_id) {
                  void qc.invalidateQueries({ queryKey: queryKeys.tasks.feed(ad.task_id) });
                  void qc.invalidateQueries({ queryKey: queryKeys.tasks.artifacts(ad.task_id) });
                }
                break;
              }

              case 'config':
                void qc.invalidateQueries({ queryKey: queryKeys.config.all });
                break;

              case 'credentials':
                void qc.invalidateQueries({ queryKey: queryKeys.credentials.all });
                break;

              case 'resync':
                log.warn('[sse] resync -- invalidating all caches');
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
              log.info('[sse] reconnected -- invalidating all caches');
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
