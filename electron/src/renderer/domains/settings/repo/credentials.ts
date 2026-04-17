import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiGet, apiPost, apiDel } from '#renderer/global/providers/http';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { toast } from 'sonner';

export interface CredentialInfo {
  id: number;
  label: string;
  tokenMasked: string;
  expiresAt: number | null;
  rateLimitCooldownUntil: number | null;
  createdAt: string;
  isExpired: boolean;
  isRateLimited: boolean;
}

interface CredentialListResponse {
  credentials: CredentialInfo[];
}

const QUERY_KEY = queryKeys.credentials.all;

export function useCredentialsList() {
  return useQuery<CredentialListResponse>({
    queryKey: QUERY_KEY,
    queryFn: () => apiGet<CredentialListResponse>('/api/credentials'),
    refetchInterval: 30_000,
  });
}

export function useCredentialAdd() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ label, token }: { label: string; token: string }) =>
      apiPost<{ ok: boolean; id?: number; label?: string }>('/api/credentials/setup-token', {
        label,
        token,
      }),
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
    mutationFn: (id: number) => apiDel(`/api/credentials/${id}`),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: QUERY_KEY });
      toast.success('Credential removed');
    },
    onError: () => toast.error('Failed to remove credential'),
  });
}

export function useCredentialReveal() {
  return useMutation({
    mutationFn: (id: number) => apiGet<{ token: string }>(`/api/credentials/${id}/token`),
  });
}
