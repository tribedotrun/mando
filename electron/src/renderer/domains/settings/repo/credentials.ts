import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiDeleteRouteR, apiGetRouteR, apiPostRouteR } from '#renderer/global/providers/http';
import { toReactQuery } from '#result';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import type {
  CredentialInfo,
  CredentialListResponse,
  CredentialRateLimitStatus,
  CredentialWindowInfo,
} from '#shared/daemon-contract';
import { toast } from '#renderer/global/runtime/useFeedback';

export type { CredentialInfo, CredentialRateLimitStatus, CredentialWindowInfo };

const QUERY_KEY = queryKeys.credentials.all;

export function useCredentialsList() {
  return useQuery<CredentialListResponse>({
    queryKey: QUERY_KEY,
    queryFn: () => toReactQuery(apiGetRouteR('getCredentials')),
    refetchInterval: 30_000,
  });
}

export function useCredentialAdd() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ label, token }: { label: string; token: string }) =>
      toReactQuery(apiPostRouteR('postCredentialsSetuptoken', { label, token })),
    onSuccess: (res) => {
      void qc.invalidateQueries({ queryKey: QUERY_KEY });
      toast.success(`Credential added: ${res.label}`);
    },
    onError: () => toast.error('Failed to add credential'),
  });
}

export function useCredentialRemove() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: number) =>
      toReactQuery(apiDeleteRouteR('deleteCredentialsById', { params: { id } })),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: QUERY_KEY });
      toast.success('Credential removed');
    },
    onError: () => toast.error('Failed to remove credential'),
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
    onError: (err: Error) => {
      toast.error(err.message || 'Failed to probe credential');
      void qc.invalidateQueries({ queryKey: QUERY_KEY });
    },
  });
}
