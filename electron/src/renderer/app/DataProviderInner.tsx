import React, { useMemo, useState } from 'react';
import { subscribeObsDegraded } from '#renderer/global/providers/obsHealth';
import { hasConfig } from '#renderer/global/providers/native/app';
import log from '#renderer/global/service/logger';
import { useDesktopNotifications } from '#renderer/global/runtime/useDesktopNotifications';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import { useNativeActions } from '#renderer/global/runtime/useNativeActions';
import { toast } from '#renderer/global/runtime/useFeedback';
import { RetryButton } from '#renderer/domains/captain/ui/RetryButton';
import { Skeleton } from '#renderer/global/ui/skeleton';
import { useSseSync } from '#renderer/global/runtime/useSseSync';
import type { SSEConnectionStatus } from '#renderer/global/types';
import { DataContext } from '#renderer/global/runtime/dataContext';
import { OnboardingPlaceholder } from '#renderer/domains/onboarding/ui/OnboardingPlaceholder';

const INIT_FALLBACK_MS = 3_000;

export function DataProviderInner({
  children,
  resetDataPlane,
}: {
  children: React.ReactNode;
  resetDataPlane: () => void;
}): React.ReactElement {
  const [initialized, setInitialized] = useState(false);
  const [initError, setInitError] = useState<string | null>(null);
  const [needsOnboarding, setNeedsOnboarding] = useState(false);
  const [sseStatus, setSseStatus] = useState<SSEConnectionStatus>('disconnected');
  const { restartDaemon } = useNativeActions();
  const { processEvent: processNotification } = useDesktopNotifications();

  useMountEffect(() =>
    subscribeObsDegraded(() => {
      toast.error('Observability pipeline degraded -- logs not being sent');
    }),
  );

  useSseSync({
    onStatusChange: (status) => {
      setSseStatus(status);
      if (status === 'connected') {
        setInitError(null);
        if (!initialized) {
          setInitialized(true);
        }
      }
    },
    onBootstrap: async () => {
      const ready = await hasConfig();
      if (!ready) {
        setNeedsOnboarding(true);
        setInitialized(true);
        return true;
      }
      return false;
    },
    processDesktopNotification: processNotification,
    onError: (msg) => {
      setInitError(msg);
      setInitialized(true);
    },
  });

  useMountEffect(() => {
    const timeoutId = setTimeout(() => {
      setInitialized((prev) => prev || true);
    }, INIT_FALLBACK_MS);

    return () => clearTimeout(timeoutId);
  });

  const contextValue = useMemo(() => ({ sseStatus, resetDataPlane }), [resetDataPlane, sseStatus]);

  const handleRestartDaemon = () => {
    void restartDaemon()
      .then(() => {
        setInitError(null);
        setNeedsOnboarding(false);
        setInitialized(false);
        resetDataPlane();
      })
      .catch((err) => {
        log.error('[DataProvider] restartDaemon failed:', err);
      });
  };

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
        <span className="max-w-full text-body text-muted-foreground [overflow-wrap:anywhere]">
          {initError}
        </span>
        <RetryButton
          className="mt-2 inline-flex items-center justify-center rounded-md bg-foreground px-4 py-2 text-sm font-medium text-background hover:bg-foreground/90"
          onRetry={handleRestartDaemon}
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
