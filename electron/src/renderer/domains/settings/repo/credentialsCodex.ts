import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiGetRouteR, apiPostRouteR } from '#renderer/global/providers/http';
import log from '#renderer/global/service/logger';
import { toReactQuery } from '#result';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { daemonSyncMeta } from '#renderer/global/repo/syncPolicy';
import type { CodexActiveResponse } from '#shared/daemon-contract';

const LIST_KEY = queryKeys.credentials.all;
const ACTIVE_KEY = queryKeys.credentials.codexActive();

/**
 * Reads `~/.codex/auth.json` on every poll and returns its account_id plus
 * the matching stored credential id (if any). The badge "Active" in the
 * Settings UI is computed from this — never stored — so a manual
 * `codex login` outside Mando is reflected without flag rot.
 *
 * On query failure, react-query retains the last successful response on
 * `data`, so a transient daemon hiccup does not flicker the Active badge.
 * The first-load-failure case (no `data` yet + error) is logged here so an
 * operator inspecting devtools sees the underlying cause rather than a
 * silently empty badge.
 */
export function useCodexActiveCredential() {
  const query = useQuery<CodexActiveResponse>({
    queryKey: ACTIVE_KEY,
    meta: daemonSyncMeta('polling', 'reflects external `codex login` writes to ~/.codex/auth.json'),
    queryFn: () => toReactQuery(apiGetRouteR('getCredentialsCodexActive')),
    refetchInterval: 30_000,
  });
  if (query.isError && !query.data) {
    log.error('codex-active query failed without prior data', query.error);
  }
  return query;
}

export function useCodexCredentialAdd() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ label, authJson }: { label: string; authJson: string }) =>
      toReactQuery(apiPostRouteR('postCredentialsCodex', { label, authJson })),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: LIST_KEY });
      void qc.invalidateQueries({ queryKey: ACTIVE_KEY });
    },
  });
}

export function useCodexCredentialActivate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: number) =>
      toReactQuery(
        apiPostRouteR('postCredentialsByIdCodexactivate', undefined, {
          params: { id },
        }),
      ),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: LIST_KEY });
      void qc.invalidateQueries({ queryKey: ACTIVE_KEY });
    },
  });
}
