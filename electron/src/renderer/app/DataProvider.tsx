import React, { createContext, use, useMemo, useState, useCallback, useRef } from 'react';
import { QueryClientProvider } from '@tanstack/react-query';
import { queryClient, invalidateTaskDetail } from '#renderer/queryClient';
import { initBaseUrl, connectSSE, OBS_DEGRADED_EVENT } from '#renderer/api';
import log from '#renderer/logger';
import { useTaskStore } from '#renderer/domains/captain/stores/taskStore';
import { useScoutStore } from '#renderer/domains/scout/stores/scoutStore';
import { useWorkbenchStore } from '#renderer/domains/terminal';
import {
  useDesktopNotifications,
  parseNotification,
} from '#renderer/global/hooks/useDesktopNotifications';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { toast } from 'sonner';
import { getErrorMessage } from '#renderer/utils';
import { RetryButton } from '#renderer/domains/captain/components/RetryButton';
import { Skeleton } from '#renderer/components/ui/skeleton';
import type { SSEConnectionStatus } from '#renderer/types';

const POLL_INTERVAL_MS = 30_000;

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
  const workbenchFetch = useWorkbenchStore((s) => s.fetch);
  const { processEvent: processNotification } = useDesktopNotifications();

  const startPolling = useCallback(() => {
    if (pollingRef.current) return;
    pollingRef.current = setInterval(() => {
      void taskFetch();
      void queryClient.invalidateQueries({ queryKey: ['sessions'] });
    }, POLL_INTERVAL_MS);
  }, [taskFetch]);

  const stopPolling = useCallback(() => {
    if (pollingRef.current) {
      clearInterval(pollingRef.current);
      pollingRef.current = null;
    }
  }, []);

  /** Refetch all stores — used on SSE reconnect to clear stale data. */
  const refetchAll = useCallback(() => {
    void taskFetch();
    void scoutFetch();
    void workbenchFetch();
    void queryClient.invalidateQueries();
  }, [taskFetch, scoutFetch, workbenchFetch]);

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
        const seedResults = await Promise.allSettled([taskFetch(), scoutFetch(), workbenchFetch()]);
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
                void taskFetch();
                void workbenchFetch();
                void queryClient.invalidateQueries({ queryKey: ['metrics-workers'] });
                invalidateTaskDetail(queryClient);
                break;
              case 'scout':
                void scoutFetch();
                void queryClient.invalidateQueries({ queryKey: ['scout'] });
                break;
              case 'status':
                void taskFetch();
                void queryClient.invalidateQueries({ queryKey: ['status'] });
                void queryClient.invalidateQueries({ queryKey: ['metrics-workers'] });
                invalidateTaskDetail(queryClient);
                break;
              case 'sessions':
                void queryClient.invalidateQueries({ queryKey: ['sessions'] });
                void queryClient.invalidateQueries({ queryKey: ['task-detail-timeline'] });
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
    void init();
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
      <div className="flex h-screen items-center justify-center bg-background">
        <div className="flex flex-col items-center gap-3">
          <Skeleton className="h-5 w-32" />
          <Skeleton className="h-4 w-20" />
        </div>
      </div>
    );
  }

  if (initError) {
    return (
      <div className="flex h-screen flex-col items-center justify-center gap-3 bg-background p-6 text-foreground">
        <span className="text-heading text-destructive">Could not connect to daemon</span>
        <span className="text-body text-muted-foreground">{initError}</span>
        <RetryButton
          className="mt-2 inline-flex items-center justify-center rounded-md bg-foreground px-4 py-2 text-sm font-medium text-background hover:bg-foreground/90"
          onRetry={() =>
            void window.mandoAPI.restartDaemon().finally(() => window.location.reload())
          }
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
    return <div className="p-6 text-destructive">Failed to load onboarding. Restart the app.</div>;
  }
  if (!OnboardingWizard) return <div />;
  return <OnboardingWizard />;
}
