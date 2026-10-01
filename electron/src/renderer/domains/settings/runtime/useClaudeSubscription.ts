import { useClaudeDesktopProfileStatus } from '#renderer/domains/settings/repo/claudeDesktopApp';
import {
  useClaudeDesktopAuthorizeKeychain,
  useClaudeSubscriptionRefresh,
} from '#renderer/domains/settings/repo/claudeSubscription';
import { useIsMutating } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';

export function useClaudeSubscription(credentialId: number) {
  const refresh = useClaudeSubscriptionRefresh(credentialId);
  const authorize = useClaudeDesktopAuthorizeKeychain();
  const authorizing =
    useIsMutating({ mutationKey: queryKeys.claudeDesktopApp.authorizeKeychain() }) > 0;
  return {
    profile: useClaudeDesktopProfileStatus(credentialId),
    refresh,
    authorize,
    authorizing,
    refreshPlan: () => {
      authorize.reset();
      refresh.mutate();
    },
    allowKeychainAccess: () =>
      authorize.mutate(undefined, {
        onSuccess: (result) => {
          if (result.authorized) refresh.mutate();
        },
      }),
  };
}
