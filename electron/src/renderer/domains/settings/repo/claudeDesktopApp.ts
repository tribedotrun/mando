import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiGetRouteR, apiPostRouteR } from '#renderer/global/providers/http';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { daemonSyncMeta } from '#renderer/global/repo/syncPolicy';
import { toReactQuery } from '#result';
import type { ClaudeDesktopSessionImportRequest } from '#shared/daemon-contract';

export function useClaudeDesktopProfileStatus(credentialId: number) {
  return useQuery({
    queryKey: queryKeys.claudeDesktopApp.status(credentialId),
    meta: daemonSyncMeta('polling', 'Claude Desktop login and process state change outside Mando'),
    queryFn: () =>
      toReactQuery(apiGetRouteR('getCredentialsClaudeDesktopStatus', { query: { credentialId } })),
    refetchInterval: 5000,
  });
}

export function useClaudeDesktopSetup() {
  const qc = useQueryClient();
  return useMutation({
    mutationKey: queryKeys.claudeDesktopApp.action(),
    mutationFn: (credentialId: number) =>
      toReactQuery(apiPostRouteR('postCredentialsClaudeDesktopSetup', { credentialId })),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.claudeDesktopApp.all });
    },
  });
}

export function useClaudeDesktopOpen() {
  const qc = useQueryClient();
  return useMutation({
    mutationKey: queryKeys.claudeDesktopApp.action(),
    mutationFn: (credentialId: number) =>
      toReactQuery(apiPostRouteR('postCredentialsClaudeDesktopOpen', { credentialId })),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.claudeDesktopApp.all });
    },
  });
}

export function useClaudeDesktopAdopt() {
  const qc = useQueryClient();
  return useMutation({
    mutationKey: queryKeys.claudeDesktopApp.action(),
    mutationFn: (body: { credentialId: number; userDataDir: string }) =>
      toReactQuery(apiPostRouteR('postCredentialsClaudeDesktopAdopt', body)),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.claudeDesktopApp.all });
    },
  });
}

export function useClaudeDesktopSessionImport() {
  const qc = useQueryClient();
  return useMutation({
    mutationKey: queryKeys.claudeDesktopApp.action(),
    mutationFn: (body: ClaudeDesktopSessionImportRequest) =>
      toReactQuery(apiPostRouteR('postCredentialsClaudeDesktopSyncsessions', body)),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.claudeDesktopApp.all });
    },
  });
}

export function useClaudeDesktopSessionPreview() {
  return useMutation({
    mutationKey: queryKeys.claudeDesktopApp.action(),
    mutationFn: (body: ClaudeDesktopSessionImportRequest) =>
      toReactQuery(apiPostRouteR('postCredentialsClaudeDesktopPreviewsessions', body)),
  });
}
