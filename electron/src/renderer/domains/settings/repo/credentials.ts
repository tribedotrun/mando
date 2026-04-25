import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiDeleteRouteR, apiGetRouteR, apiPostRouteR } from '#renderer/global/providers/http';
import { toReactQuery } from '#result';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { daemonSyncMeta } from '#renderer/global/repo/syncPolicy';
import type {
  CredentialInfo,
  CredentialListResponse,
  CredentialRateLimitStatus,
  CredentialWindowInfo,
} from '#shared/daemon-contract';

export type { CredentialInfo, CredentialRateLimitStatus, CredentialWindowInfo };

const QUERY_KEY = queryKeys.credentials.all;

export function useCredentialsList() {
  return useQuery<CredentialListResponse>({
    queryKey: QUERY_KEY,
    meta: daemonSyncMeta('polling', 'credential rate-limit state changes over time'),
    queryFn: () => toReactQuery(apiGetRouteR('getCredentials')),
    refetchInterval: 30_000,
  });
}

export function useCredentialAdd() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ label, token }: { label: string; token: string }) =>
      toReactQuery(apiPostRouteR('postCredentialsSetuptoken', { label, token })),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: QUERY_KEY });
    },
  });
}

export function useCredentialRemove() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: number) =>
      toReactQuery(apiDeleteRouteR('deleteCredentialsById', { params: { id } })),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: QUERY_KEY });
    },
  });
}

export function useCredentialReveal() {
  return useMutation({
    mutationFn: (id: number) =>
      toReactQuery(apiGetRouteR('getCredentialsByIdToken', { params: { id } })),
  });
}

export function useCredentialProbe() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: number) =>
      toReactQuery(apiPostRouteR('postCredentialsByIdProbe', undefined, { params: { id } })),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: QUERY_KEY });
    },
    onError: () => {
      void qc.invalidateQueries({ queryKey: QUERY_KEY });
    },
  });
}
