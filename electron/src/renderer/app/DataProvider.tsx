import React, { createContext, use, useMemo, useState } from 'react';
import { QueryClientProvider } from '@tanstack/react-query';
import { queryClient } from '#renderer/queryClient';
import { OBS_DEGRADED_EVENT } from '#renderer/api';
import log from '#renderer/logger';
import { useDesktopNotifications } from '#renderer/global/hooks/useDesktopNotifications';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { toast } from 'sonner';
import { RetryButton } from '#renderer/domains/captain/components/RetryButton';
import { Skeleton } from '#renderer/components/ui/skeleton';
import { useSseSync } from '#renderer/hooks/useSseSync';
import type { SSEConnectionStatus } from '#renderer/types';

const INIT_FALLBACK_MS = 3_000;

// ---------------------------------------------------------------------------
// Context -- exposes SSE status to consumers
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
// Inner component (rendered inside QueryClientProvider so hooks work)
// ---------------------------------------------------------------------------

function DataProviderInner({ children }: { children: React.ReactNode }): React.ReactElement {
  const [initialized, setInitialized] = useState(false);
  const [initError, setInitError] = useState<string | null>(null);
  const [needsOnboarding, setNeedsOnboarding] = useState(false);
  const [sseStatus, setSseStatus] = useState<SSEConnectionStatus>('disconnected');

  const { processEvent: processNotification } = useDesktopNotifications();

  // OBS degraded listener
  useMountEffect(() => {
    const onObsDegraded = () => {
      toast.error('Observability pipeline degraded -- logs not being sent');
    };
    window.addEventListener(OBS_DEGRADED_EVENT, onObsDegraded);
    return () => {
      window.removeEventListener(OBS_DEGRADED_EVENT, onObsDegraded);
    };
  });

  // SSE sync -- handles snapshot seeding, tier 1/2 patching, reconnect recovery
  useSseSync({
    onStatusChange: (status) => {
      setSseStatus(status);
      if (status === 'connected' && !initialized) {
        setInitialized(true);
      }
    },
    onBootstrap: async () => {
      if (window.mandoAPI) {
        const hasConfig = await window.mandoAPI.hasConfig();
        if (!hasConfig) {
          setNeedsOnboarding(true);
          setInitialized(true);
          return true; // needs onboarding, skip SSE
        }
      }
      return false;
    },
    processDesktopNotification: processNotification,
    onError: (msg) => {
      setInitError(msg);
      setInitialized(true);
    },
  });

  // Mark initialized once the hook has had a chance to run (snapshot seeds on first connect)
  useMountEffect(() => {
    // If SSE connects quickly the onStatusChange callback sets initialized.
    // As a fallback, mark initialized after a short delay so the UI doesn't
    // stay on the skeleton forever if the first status event hasn't fired yet.
    const t = setTimeout(() => {
      setInitialized((prev) => prev || true);
    }, INIT_FALLBACK_MS);
    return () => clearTimeout(t);
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
    <DataContext value={contextValue}>
      {needsOnboarding ? <OnboardingPlaceholder /> : children}
    </DataContext>
  );
}

// ---------------------------------------------------------------------------
// Provider (outermost shell)
// ---------------------------------------------------------------------------

export function DataProvider({ children }: { children: React.ReactNode }): React.ReactElement {
  return (
    <QueryClientProvider client={queryClient}>
      <DataProviderInner>{children}</DataProviderInner>
    </QueryClientProvider>
  );
}

/** Thin wrapper so the onboarding import stays lazy in App.tsx */
function OnboardingPlaceholder(): React.ReactElement {
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
