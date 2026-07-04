import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiGetRouteR, apiPostRouteR } from '#renderer/global/providers/http';
import { toReactQuery } from '#result';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { daemonSyncMeta } from '#renderer/global/repo/syncPolicy';
import type { CodexResetCreditsResponse } from '#shared/daemon-contract';

export type { CodexResetCreditsResponse };

const LIST_KEY = queryKeys.credentials.all;

export function useCodexCredentialAdd() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ label, authJson }: { label: string; authJson: string }) =>
      toReactQuery(apiPostRouteR('postCredentialsCodex', { label, authJson })),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: LIST_KEY });
    },
  });
}

export function useCodexResetCredits(id: number, enabled: boolean) {
  return useQuery<CodexResetCreditsResponse>({
    queryKey: queryKeys.credentials.codexResetCredits(id),
    enabled,
    meta: daemonSyncMeta('polling', 'Codex reset credits expire outside daemon'),
    queryFn: () =>
      toReactQuery(apiGetRouteR('getCredentialsCodexByIdResetcredits', { params: { id } })),
    staleTime: 300_000,
    refetchInterval: enabled ? 300_000 : false,
  });
}
