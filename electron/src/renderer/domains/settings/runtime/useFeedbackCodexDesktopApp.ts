import { useCallback } from 'react';
import { useIsMutating } from '@tanstack/react-query';
import { toast } from '#renderer/global/runtime/useFeedback';
import { getErrorMessage } from '#renderer/global/service/utils';
import log from '#renderer/global/service/logger';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import {
  useCodexAppRestoreMutation,
  useCodexAppUseMutation,
  useCodexDesktopAppStatusQuery,
  type CodexDesktopAppStatus,
} from '#renderer/domains/settings/repo/codexDesktopApp';

export type { CodexDesktopAppStatus };

/** Shared in-flight signal for either swap mutation (see
 * `repo/codexDesktopApp.ts`), so every "use in desktop app" row and the
 * banner's restore button lock together during any single in-flight swap
 * instead of racing `~/.codex/auth.json` and the swap-state file. */
function useAnyCodexSwapInFlight(): boolean {
  return useIsMutating({ mutationKey: queryKeys.codexDesktopApp.swap() }) > 0;
}

interface UseCodexDesktopAppStatusResult {
  status: CodexDesktopAppStatus | null;
  restorePersonal: () => void;
  restoring: boolean;
  anySwapInFlight: boolean;
}

/**
 * Reads the shared Codex desktop-app status query and exposes the restore
 * action. Backed by React Query (see `repo/codexDesktopApp.ts`) so every
 * consumer -- this hook, `useUseInDesktopApp` below, and
 * `CodexDesktopAppBanner` -- reads the same cache entry: a swap triggered
 * from a credential row invalidates the query the banner reads, so the
 * banner updates immediately without a manually-threaded `refetch`.
 */
export function useCodexDesktopAppStatus(): UseCodexDesktopAppStatusResult {
  const { data } = useCodexDesktopAppStatusQuery();
  const restoreMutation = useCodexAppRestoreMutation();
  const anySwapInFlight = useAnyCodexSwapInFlight();

  const restorePersonal = useCallback(() => {
    void (async () => {
      try {
        await restoreMutation.mutateAsync();
        toast.success('Restored personal Codex account in the desktop app');
      } catch (err: unknown) {
        log.error('[useCodexDesktopAppStatus] restorePersonal failed:', err);
        toast.error(getErrorMessage(err, 'Failed to restore personal account'));
      }
    })();
  }, [restoreMutation]);

  return {
    status: data ?? null,
    restorePersonal,
    restoring: restoreMutation.isPending,
    anySwapInFlight,
  };
}

interface UseUseInDesktopAppResult {
  useInDesktopApp: (label: string) => Promise<void>;
  pending: boolean;
  anySwapInFlight: boolean;
}

/**
 * Drives the per-row "Use in desktop app" action through the shared
 * `codexAppUse` mutation. `pending` is this row's own mutation state (drives
 * its spinner); `anySwapInFlight` observes the mutation key shared with
 * `useCodexAppRestoreMutation`, so every row -- and the banner's restore
 * button -- can disable while any single swap is in flight, anywhere.
 */
export function useUseInDesktopApp(): UseUseInDesktopAppResult {
  const swapMutation = useCodexAppUseMutation();
  const anySwapInFlight = useAnyCodexSwapInFlight();

  const useInDesktopApp = useCallback(
    async (label: string): Promise<void> => {
      try {
        await swapMutation.mutateAsync(label);
        toast.success(`Desktop app now using ${label}`);
      } catch (err: unknown) {
        log.error('[useUseInDesktopApp] useInDesktopApp failed:', err);
        toast.error(getErrorMessage(err, 'Failed to switch the desktop app account'));
      }
    },
    [swapMutation],
  );

  return { useInDesktopApp, pending: swapMutation.isPending, anySwapInFlight };
}
