import { useMutation, useQueryClient } from '@tanstack/react-query';
import { apiPostRouteR } from '#renderer/global/providers/http';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { toReactQuery } from '#result';

export function useClaudeSubscriptionRefresh(credentialId: number) {
  const qc = useQueryClient();
  return useMutation({
    mutationKey: queryKeys.credentials.claudeSubscriptionRefresh(credentialId),
    mutationFn: () =>
      toReactQuery(
        apiPostRouteR('postCredentialsClaudeDesktopSubscriptionRefresh', { credentialId }),
      ),
    onSettled: () => qc.invalidateQueries({ queryKey: queryKeys.credentials.all }),
  });
}

export function useClaudeDesktopAuthorizeKeychain() {
  return useMutation({
    mutationKey: queryKeys.claudeDesktopApp.authorizeKeychain(),
    mutationFn: () =>
      toReactQuery(apiPostRouteR('postCredentialsClaudeDesktopAuthorizekeychain', {})),
  });
}
