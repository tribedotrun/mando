import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiGetRouteR, apiPostRouteR } from '#renderer/global/providers/http';
import { toReactQuery } from '#result';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { daemonSyncMeta } from '#renderer/global/repo/syncPolicy';
import type {
  AddCodexCredentialResponse,
  CancelCodexLoginResponse,
  CodexLoginFlowInfo,
  CodexLoginStatus,
  CodexLoginStatusResponse,
  CodexResetCreditsResponse,
  StartCodexLoginResponse,
} from '#shared/daemon-contract';

export type {
  AddCodexCredentialResponse,
  CancelCodexLoginResponse,
  CodexLoginFlowInfo,
  CodexLoginStatus,
  CodexLoginStatusResponse,
  CodexResetCreditsResponse,
  StartCodexLoginResponse,
};

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

export function useCodexLoginStart() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { label?: string | null; credentialId?: number | null }) =>
      toReactQuery(apiPostRouteR('postCredentialsCodexLoginStart', vars)),
    onSuccess: () => {
      // The status cache may still hold the pre-start flow snapshot; drop it
      // and fetch the fresh flow now instead of waiting for the next poll.
      void qc.invalidateQueries({ queryKey: queryKeys.credentials.codexLogin() });
    },
  });
}

export function useCodexLoginStatus(enabled: boolean) {
  return useQuery<CodexLoginStatusResponse>({
    queryKey: queryKeys.credentials.codexLogin(),
    enabled,
    meta: daemonSyncMeta('polling', 'browser login flow progresses in the daemon outside queries'),
    queryFn: () => toReactQuery(apiGetRouteR('getCredentialsCodexLoginCurrent')),
    refetchInterval: enabled ? 1500 : false,
  });
}

export function useCodexLoginCancel() {
  return useMutation({
    mutationFn: () => toReactQuery(apiPostRouteR('postCredentialsCodexLoginCancel', undefined)),
  });
}

export function useCodexCredentialUpdateAuth() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, authJson }: { id: number; authJson: string }) =>
      toReactQuery(apiPostRouteR('postCredentialsCodexByIdAuth', { authJson }, { params: { id } })),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: LIST_KEY });
    },
  });
}
