import React, { createContext, use, useMemo, useState, useCallback, useRef } from 'react';
import { QueryClientProvider } from '@tanstack/react-query';
import { queryClient } from '#renderer/queryClient';
import { initBaseUrl, connectSSE } from '#renderer/api';
import log from '#renderer/logger';
import { useTaskStore } from '#renderer/stores/taskStore';
import { useScoutStore } from '#renderer/stores/scoutStore';
import { useCronStore } from '#renderer/stores/cronStore';
import { useDesktopNotifications } from '#renderer/hooks/useDesktopNotifications';
import { useMountEffect } from '#renderer/hooks/useMountEffect';
import { useToastStore } from '#renderer/stores/toastStore';
import type { NotificationPayload, SSEConnectionStatus } from '#renderer/types';

// ---------------------------------------------------------------------------
// Context — exposes SSE status + sessions refresh trigger to consumers
// ---------------------------------------------------------------------------

interface DataContextValue {
  sseStatus: SSEConnectionStatus;
  sessionsRefresh: number;
}

const DataContext = createContext<DataContextValue>({
  sseStatus: 'disconnected',
  sessionsRefresh: 0,
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
  const [sessionsRefresh, setSessionsRefresh] = useState(0);
  const sseRef = useRef<EventSource | null>(null);
  const pollingRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const taskFetch = useTaskStore((s) => s.fetch);
  const scoutFetch = useScoutStore((s) => s.fetch);
  const cronFetch = useCronStore((s) => s.fetch);
  const { processEvent: processNotification } = useDesktopNotifications();

  const startPolling = useCallback(() => {
    if (pollingRef.current) return;
    pollingRef.current = setInterval(() => {
      taskFetch();
      setSessionsRefresh((n) => n + 1);
    }, 30_000);
  }, [taskFetch]);

  const stopPolling = useCallback(() => {
    if (pollingRef.current) {
      clearInterval(pollingRef.current);
      pollingRef.current = null;
    }
  }, []);

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
        await Promise.allSettled([taskFetch(), scoutFetch(), cronFetch()]);
        sseRef.current = connectSSE(
          (event) => {
            if (event.event === 'tasks') {
              taskFetch();
              queryClient.invalidateQueries({ queryKey: ['metrics-workers'] });
            }
            if (event.event === 'scout') {
              scoutFetch();
              queryClient.invalidateQueries({ queryKey: ['scout'] });
            }
            if (event.event === 'cron') {
              cronFetch();
            }
            if (event.event === 'status') {
              taskFetch();
              queryClient.invalidateQueries({ queryKey: ['status'] });
              queryClient.invalidateQueries({ queryKey: ['metrics-workers'] });
            }
            if (event.event === 'sessions') {
              queryClient.invalidateQueries({ queryKey: ['sessions'] });
              setSessionsRefresh((n) => n + 1);
            }
            // Surface rate limit events as in-app toasts.
            if (event.event === 'notification' && event.data) {
              if (
                typeof event.data !== 'object' ||
                event.data === null ||
                !('message' in event.data)
              ) {
                log.warn('[DataProvider] unexpected notification shape:', event.data);
                return;
              }
              const payload = event.data as unknown as NotificationPayload;
              if (payload.kind?.type === 'RateLimited') {
                const variant = payload.kind.status === 'rejected' ? 'error' : 'info';
                useToastStore.getState().add(variant, payload.message);
              }
            }
            processNotification(event);
          },
          (status) => {
            setSseStatus(status);
            if (status === 'disconnected') startPolling();
            else if (status === 'connected') stopPolling();
          },
        );
        setInitialized(true);
      } catch (err) {
        log.error('[DataProvider] init failed:', err);
        setInitError(err instanceof Error ? err.message : String(err));
        setInitialized(true);
      }
    };
    init();
    return () => {
      sseRef.current?.close();
      stopPolling();
    };
  });

  const contextValue = useMemo(
    () => ({ sseStatus, sessionsRefresh }),
    [sseStatus, sessionsRefresh],
  );

  if (!initialized) {
    return (
      <div
        className="flex h-screen items-center justify-center"
        style={{ background: 'var(--color-bg)', color: 'var(--color-text-3)' }}
      >
        <span className="text-body">Loading...</span>
      </div>
    );
  }

  if (initError) {
    return (
      <div
        className="flex h-screen flex-col items-center justify-center gap-3"
        style={{ background: 'var(--color-bg)', color: 'var(--color-text-1)', padding: 24 }}
      >
        <span className="text-heading" style={{ color: 'var(--color-error)' }}>
          Could not connect to daemon
        </span>
        <span className="text-body" style={{ color: 'var(--color-text-3)' }}>
          {initError}
        </span>
        <button
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
          onClick={() => window.location.reload()}
        >
          Retry
        </button>
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
    import('#renderer/components/OnboardingWizard')
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
      <div style={{ padding: 24, color: 'var(--color-error)' }}>
        Failed to load onboarding. Restart the app.
      </div>
    );
  }
  if (!OnboardingWizard) return <div />;
  return <OnboardingWizard />;
}
