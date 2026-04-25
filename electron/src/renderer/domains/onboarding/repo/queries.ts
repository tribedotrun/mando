import { useQuery } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { checkClaudeCode as checkClaudeCodeNative } from '#renderer/global/providers/native/onboarding';
import log from '#renderer/global/service/logger';
import { getErrorMessage } from '#renderer/global/service/utils';
import type { ClaudeCheckResult } from '#renderer/domains/onboarding/types';

export function useClaudeCodeCheck() {
  return useQuery<ClaudeCheckResult>({
    queryKey: queryKeys.onboarding.claudeCheck(),
    queryFn: async () => {
      try {
        return await checkClaudeCodeNative();
      } catch (err) {
        log.error('checkClaudeCode failed:', err);
        return {
          installed: false,
          version: null,
          works: false,
          checkFailed: true,
          error: getErrorMessage(err, 'Unknown error'),
        };
      }
    },
    staleTime: Infinity,
    gcTime: Infinity,
  });
}
