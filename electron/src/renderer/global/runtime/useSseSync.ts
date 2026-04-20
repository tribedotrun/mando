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
  handleStatusEvent,
  handleSessionsEvent,
  seedFromSnapshot,
} from '#renderer/global/repo/sseCacheHelpers';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import type { SSEConnectionStatus, SSEEvent } from '#renderer/global/types';
import { parseNotification } from '#renderer/global/service/notificationHelpers';
import { toast } from '#renderer/global/runtime/useFeedback';
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
                const counts = seedFromSnapshot(qc, event.data.data);
                log.debug('[sse] snapshot seeded caches', counts);
                break;
              }

              case 'snapshot_error': {
                const message = event.data.data.message;
                log.error('[sse] snapshot_error:', message);
                optionsRef.current?.onError?.(message);
                void qc.invalidateQueries();
                break;
              }

              case 'tasks': {
                const payload = event.data.data;
                if (
                  payload?.action === 'created' ||
                  payload?.action === 'updated' ||
                  payload?.action === 'deleted'
                ) {
                  patchTaskList(qc, payload);
                } else {
                  // Legacy: empty signal, fall back to invalidation
                  void qc.invalidateQueries({ queryKey: queryKeys.tasks.list() });
                  void qc.invalidateQueries({ queryKey: queryKeys.workers.list() });
                  void qc.invalidateQueries({ queryKey: queryKeys.stats.all });
                }
                break;
              }

              case 'scout': {
                const payload = event.data.data;
                if (
                  payload?.action === 'created' ||
                  payload?.action === 'updated' ||
                  payload?.action === 'deleted'
                ) {
                  patchScoutList(qc, payload);
                } else {
                  void qc.invalidateQueries({ queryKey: queryKeys.scout.all });
                }
                break;
              }

              case 'workbenches': {
                const payload = event.data.data;
                if (
                  payload?.action === 'created' ||
                  payload?.action === 'updated' ||
                  payload?.action === 'deleted'
                ) {
                  patchWorkbenchList(qc, payload);
                  // Invalidate only filtered variants (archived, all) so they refetch, but preserve the active list's Tier 1 patch-only behavior (zero HTTP refetches in normal operation).
                  void qc.invalidateQueries({
                    queryKey: queryKeys.workbenches.all,
                    predicate: (query) => query.queryKey.length > 2,
                  });
                } else {
                  void qc.invalidateQueries({ queryKey: queryKeys.workbenches.all });
                }
                break;
              }

              case 'status':
                // Outer SSE parse already narrowed to StatusPayload; data.data is typed.
                handleStatusEvent(qc, event.data.data ?? null);
                break;

              case 'sessions':
                // Outer SSE parse already narrowed to SessionsPayload; data.data is typed.
                handleSessionsEvent(qc, event.data.data ?? null);
                break;

              case 'notification': {
                const payload = parseNotification(event);
                if (payload) {
                  if (payload.kind?.type === 'RateLimited') {
                    const fn = payload.kind.status === 'rejected' ? toast.error : toast.info;
                    fn(payload.message);
                  }
                } else if (event.data.data) {
                  log.warn('[sse] unexpected notification shape:', event.data);
                }
                break;
              }

              case 'research': {
                // Outer SSE parse already narrowed to ResearchPayload; data.data is typed.
                const rd = event.data.data;
                if (rd) {
                  if (rd.action === 'completed') {
                    const added = rd.added_count ?? 0;
                    toast.success(`Research complete: ${added} link(s) added`);
                    void qc.invalidateQueries({ queryKey: queryKeys.scout.all });
                  } else if (rd.action === 'failed') {
                    const err = rd.error ?? 'Unknown error';
                    toast.error(`Research failed: ${err}`);
                    void qc.invalidateQueries({ queryKey: queryKeys.scout.research() });
                  } else if (rd.action === 'started') {
                    void qc.invalidateQueries({ queryKey: queryKeys.scout.research() });
                  } else if (rd.action === 'progress') {
                    void qc.invalidateQueries({ queryKey: queryKeys.scout.research() });
                  }
                }
                break;
              }

              case 'artifacts': {
                // Outer SSE parse already narrowed to ArtifactsPayload; data.data is typed.
                const ad = event.data.data;
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

              default: {
                const unexpected: never = event;
                log.error('[sse] unexpected daemon event', unexpected);
                void qc.invalidateQueries();
                break;
              }
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
