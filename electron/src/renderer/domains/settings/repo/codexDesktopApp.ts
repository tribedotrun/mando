/**
 * React Query wrappers for the Codex desktop-app (ChatGPT/Codex Electron
 * app) account-swap status and actions. These call native IPC wrappers in
 * `global/providers/native/app.ts`; Electron main forwards them through the
 * generated daemon contract. The query and mutations belong in repo/: every consumer (the
 * credential row's "use in desktop app" button, the active-swap warning
 * banner) needs to share one cache entry and one in-flight signal instead
 * of each mounting its own local status poll and pending flag.
 *
 * Both mutations share `queryKeys.codexDesktopApp.swap()` as their
 * `mutationKey` so `useIsMutating({ mutationKey })` in the runtime layer
 * observes either one -- a swap started from any credential row is visible
 * to every other row and to the banner's restore button.
 */
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  getCodexDesktopAppStatus,
  restoreCodexDesktopApp,
  useCodexInDesktopApp,
} from '#renderer/global/providers/native/app';
import { queryKeys } from '#renderer/global/repo/queryKeys';

export type CodexDesktopAppStatus = Awaited<ReturnType<typeof getCodexDesktopAppStatus>>;

export function useCodexDesktopAppStatusQuery() {
  return useQuery<CodexDesktopAppStatus>({
    queryKey: queryKeys.codexDesktopApp.status(),
    queryFn: () => getCodexDesktopAppStatus(),
  });
}

export function useCodexAppUseMutation() {
  const qc = useQueryClient();
  return useMutation({
    mutationKey: queryKeys.codexDesktopApp.swap(),
    mutationFn: (label: string) => useCodexInDesktopApp(label),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.codexDesktopApp.status() });
    },
  });
}

export function useCodexAppRestoreMutation() {
  const qc = useQueryClient();
  return useMutation({
    mutationKey: queryKeys.codexDesktopApp.swap(),
    mutationFn: () => restoreCodexDesktopApp(),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.codexDesktopApp.status() });
    },
  });
}
