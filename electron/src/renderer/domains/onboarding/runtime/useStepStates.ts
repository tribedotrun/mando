import { useCallback, useEffect } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import {
  useConfig,
  useConfigSave,
  useConfigSnapshot,
} from '#renderer/domains/onboarding/runtime/hooks';
import { useClaudeCodeCheck } from '#renderer/domains/onboarding/repo/queries';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import log from '#renderer/global/service/logger';
import type { MandoConfig } from '#renderer/global/types';
import type { ClaudeCheckResult } from '#renderer/domains/onboarding/service/types';

export function useStepStates() {
  const { data: config } = useConfig();
  const saveMut = useConfigSave();
  const getConfig = useConfigSnapshot();
  const queryClient = useQueryClient();
  const hasBotToken = !!config?.env?.TELEGRAM_MANDO_BOT_TOKEN;
  const { data: claudeResult = null } = useClaudeCodeCheck();

  const persistClaudeOk = useCallback(
    (result: ClaudeCheckResult) => {
      if (result.installed && result.works) {
        if (!config?.features?.claudeCodeVerified) {
          const current = getConfig();
          if (!current) return;
          const updated: MandoConfig = {
            ...current,
            features: { ...(current.features || {}), claudeCodeVerified: true },
          };
          saveMut.mutate(updated, {
            onError: (err) => log.warn('persistClaudeOk save failed:', err),
          });
        }
      }
    },
    [config?.features?.claudeCodeVerified, saveMut, getConfig],
  );

  // eslint-disable-next-line no-restricted-syntax -- reason: reacting to async query data requires an effect; persist runs exactly when claudeResult transitions to ok
  useEffect(() => {
    if (claudeResult) persistClaudeOk(claudeResult);
  }, [claudeResult, persistClaudeOk]);

  const recheckClaude = useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: queryKeys.onboarding.claudeCheck() });
  }, [queryClient]);

  const claudeOk =
    (claudeResult?.installed === true && claudeResult.works === true) ||
    config?.features?.claudeCodeVerified === true;

  return {
    claudeResult,
    claudeOk,
    recheckClaude,
    hasProject: Object.keys(config?.captain?.projects ?? {}).length > 0,
    hasTelegram: !!(config?.channels?.telegram?.enabled && hasBotToken),
  };
}
