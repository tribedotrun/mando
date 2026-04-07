import React, { createContext, use, useMemo, useState, useCallback, useRef } from 'react';
import { QueryClientProvider } from '@tanstack/react-query';
import { queryClient, invalidateTaskDetail } from '#renderer/queryClient';
import { initBaseUrl, connectSSE, OBS_DEGRADED_EVENT } from '#renderer/api';
import log from '#renderer/logger';
import { useTaskStore } from '#renderer/domains/captain/stores/taskStore';
import { useScoutStore } from '#renderer/domains/scout/stores/scoutStore';
import {
  useDesktopNotifications,
  parseNotification,
} from '#renderer/global/hooks/useDesktopNotifications';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { toast } from 'sonner';
import { getErrorMessage } from '#renderer/utils';
import { RetryButton } from '#renderer/domains/captain/components/RetryButton';
import type { SSEConnectionStatus } from '#renderer/types';

// ---------------------------------------------------------------------------
// Context — exposes SSE status + sessions refresh trigger to consumers
// ---------------------------------------------------------------------------

interface DataContextValue {
  sseStatus: SSEConnectionStatus;
}

const DataContext = createContext<DataContextValue>({
  sseStatus: 'disconnected',
});

export function useDataContext(): DataContextValue {
  return use(DataContext);
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

export function DataProvider({ children }: { children: React.ReactNode }): React.ReactElement {
  const [initialized, setInitialized] = useState(false);
  const [initError, setInitError] = useState<string | null>(null);
  const [needsOnboarding, setNeedsOnboarding] = useState(false);
  const [sseStatus, setSseStatus] = useState<SSEConnectionStatus>('disconnected');
  const sseRef = useRef<EventSource | null>(null);
  const pollingRef = useRef<ReturnType<typeof setInterval> | null>(null);
  // Start as 'connected' so the first SSE connect doesn't trigger a redundant
  // refetchAll — the init sequence already seeds all stores before connecting.
  const prevSseStatusRef = useRef<SSEConnectionStatus>('connected');

  const taskFetch = useTaskStore((s) => s.fetch);
  const scoutFetch = useScoutStore((s) => s.fetch);
  const { processEvent: processNotification } = useDesktopNotifications();

  const startPolling = useCallback(() => {
    if (pollingRef.current) return;
    pollingRef.current = setInterval(() => {
      taskFetch();
      queryClient.invalidateQueries({ queryKey: ['sessions'] });
    }, 30_000);
  }, [taskFetch]);

  const stopPolling = useCallback(() => {
    if (pollingRef.current) {
      clearInterval(pollingRef.current);
      pollingRef.current = null;
    }
  }, []);

  /** Refetch all stores — used on SSE reconnect to clear stale data. */
  const refetchAll = useCallback(() => {
    taskFetch();
    scoutFetch();
    queryClient.invalidateQueries();
  }, [taskFetch, scoutFetch]);

  useMountEffect(() => {
    const init = async () => {
      try {
        await initBaseUrl();
        if (window.mandoAPI) {
          const hasConfig = await window.mandoAPI.hasConfig();
          if (!hasConfig) {
            setNeedsOnboarding(true);
            setInitialized(true);
            return;
          }
        }
        // Seed initial data — partial failures are OK, SSE will fill gaps
        const seedResults = await Promise.allSettled([taskFetch(), scoutFetch()]);
        const rejected = seedResults
          .map((r, i) => ({ r, label: i === 0 ? 'tasks' : 'scout' }))
          .filter(({ r }) => r.status === 'rejected');
        const storeErrors: Array<{ label: string; message: string }> = [];
        const taskError = useTaskStore.getState().error;
        if (taskError) storeErrors.push({ label: 'tasks', message: taskError });
        const scoutError = useScoutStore.getState().error;
        if (scoutError) storeErrors.push({ label: 'scout', message: scoutError });

        const rejectedMessages = rejected.map(({ r, label }) => {
          const reason = r.status === 'rejected' ? r.reason : null;
          return { label, message: getErrorMessage(reason, 'unknown') };
        });
        const failures = [...rejectedMessages, ...storeErrors];

        if (failures.length >= 2) {
          log.error('[DataProvider] initial seed failed for all stores:', failures);
          toast.error(`Failed to load initial data: ${failures[0].message}`);
        } else if (failures.length === 1) {
          log.warn('[DataProvider] initial seed had partial failures:', failures);
          toast.error(`Failed to load ${failures[0].label}: ${failures[0].message}`);
        }
        sseRef.current = connectSSE(
          (event) => {
            switch (event.event) {
              case 'tasks':
                taskFetch();
                queryClient.invalidateQueries({ queryKey: ['metrics-workers'] });
                invalidateTaskDetail(queryClient);
                break;
              case 'scout':
                scoutFetch();
                queryClient.invalidateQueries({ queryKey: ['scout'] });
                break;
              case 'status':
                taskFetch();
                queryClient.invalidateQueries({ queryKey: ['status'] });
                queryClient.invalidateQueries({ queryKey: ['metrics-workers'] });
                invalidateTaskDetail(queryClient);
                break;
              case 'sessions':
                queryClient.invalidateQueries({ queryKey: ['sessions'] });
                queryClient.invalidateQueries({ queryKey: ['task-detail-timeline'] });
                break;
              case 'notification': {
                const payload = parseNotification(event);
                if (payload) {
                  if (payload.kind?.type === 'RateLimited') {
                    const fn = payload.kind.status === 'rejected' ? toast.error : toast.info;
                    fn(payload.message);
                  }
                } else if (event.data) {
                  log.warn('[DataProvider] unexpected notification shape:', event.data);
                }
                break;
              }
              case 'resync':
                // Gateway broadcast lagged. Refetch everything to recover from
                // missed deltas; surface a toast so the user knows what happened.
                log.warn('[DataProvider] SSE resync requested, refetching all data');
                toast.info('Connection caught up, reloading data');
                refetchAll();
                break;
            }
            processNotification(event);
          },
          (status) => {
            const wasDisconnected = prevSseStatusRef.current === 'disconnected';
            prevSseStatusRef.current = status;
            setSseStatus(status);
            if (status === 'disconnected') {
              startPolling();
            } else if (status === 'connected') {
              stopPolling();
              // Reconnected after disconnect — refetch everything to clear stale data
              if (wasDisconnected) refetchAll();
            }
          },
        );
        setInitialized(true);
      } catch (err) {
        log.error('[DataProvider] init failed:', err);
        setInitError(getErrorMessage(err, 'Unknown daemon connection error'));
        setInitialized(true);
      }
    };
    init();
    const onObsDegraded = () => {
      toast.error('Observability pipeline degraded — logs not being sent');
    };
    window.addEventListener(OBS_DEGRADED_EVENT, onObsDegraded);
    return () => {
      sseRef.current?.close();
      stopPolling();
      window.removeEventListener(OBS_DEGRADED_EVENT, onObsDegraded);
    };
  });

  const contextValue = useMemo(() => ({ sseStatus }), [sseStatus]);

  if (!initialized) {
    return (
      <div className="flex h-screen items-center justify-center bg-bg text-text-3">
        <span className="text-body">Loading...</span>
      </div>
    );
  }

  if (initError) {
    return (
      <div
        className="flex h-screen flex-col items-center justify-center gap-3 bg-bg text-text-1"
        style={{ padding: 24 }}
      >
        <span className="text-heading text-error">Could not connect to daemon</span>
        <span className="text-body text-text-3">{initError}</span>
        <RetryButton
          className="text-label"
          style={{
            marginTop: 8,
            padding: '6px 16px',
            background: 'var(--color-accent)',
            color: 'var(--color-bg)',
            border: 'none',
            borderRadius: 6,
            cursor: 'pointer',
          }}
          onRetry={() => window.mandoAPI.restartDaemon().finally(() => window.location.reload())}
        />
      </div>
    );
  }

  return (
    <QueryClientProvider client={queryClient}>
      <DataContext value={contextValue}>
        {needsOnboarding ? <OnboardingPlaceholder /> : children}
      </DataContext>
    </QueryClientProvider>
  );
}

/** Thin wrapper so the onboarding import stays lazy in App.tsx */
function OnboardingPlaceholder(): React.ReactElement {
  // Dynamic import would be ideal, but keep it simple: render a placeholder
  // that App.tsx replaces once it mounts. For now, inline the component.
  const [OnboardingWizard, setOW] = useState<React.ComponentType | null>(null);
  const [loadError, setLoadError] = useState(false);
  useMountEffect(() => {
    import('#renderer/domains/onboarding/components/OnboardingWizard')
      .then((mod) => {
        setOW(() => mod.OnboardingWizard);
      })
      .catch((err) => {
        log.error('[onboarding] chunk load failed:', err);
        setLoadError(true);
      });
  });
  if (loadError) {
    return (
      <div className="text-error" style={{ padding: 24 }}>
        Failed to load onboarding. Restart the app.
      </div>
    );
  }
  if (!OnboardingWizard) return <div />;
  return <OnboardingWizard />;
}
