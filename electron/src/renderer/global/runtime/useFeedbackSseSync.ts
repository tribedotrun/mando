/**
 * SSE-to-cache bridge: connects to the daemon's SSE stream and keeps the
 * React Query cache in sync via helpers from sseCacheHelpers.
 */

import { useCallback, useRef } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { connectSSE, initBaseUrl } from '#renderer/global/providers/http';
import { invalidateAllDaemonQueries } from '#renderer/global/repo/syncPolicy';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import { handleFeedbackSseEvent } from '#renderer/global/runtime/useFeedbackSseEvents';
import { routeSseEvent } from '#renderer/global/runtime/sseEventRouter';
import type { SSEConnectionStatus, SSEEvent } from '#renderer/global/types';
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
          (event) => {
            routeSseEvent({
              event,
              queryClient: qc,
              onError: optionsRef.current?.onError,
              processDesktopNotification: optionsRef.current?.processDesktopNotification,
            });
            handleFeedbackSseEvent(event);
          },
          (newStatus: SSEConnectionStatus) => {
            const wasDisconnected = prevStatusRef.current === 'disconnected';
            prevStatusRef.current = newStatus;
            setStatus(newStatus);

            if (newStatus === 'connected' && wasDisconnected) {
              // Reconnect: full invalidation to catch up on missed events
              log.info('[sse] reconnected -- invalidating all caches');
              invalidateAllDaemonQueries(qc, 'reconnect-catchup');
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
