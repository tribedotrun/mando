import { useState, useCallback } from 'react';
import {
  useConfig,
  useConfigSave,
  useConfigSnapshot,
} from '#renderer/domains/onboarding/runtime/hooks';
import type { MandoConfig } from '#renderer/global/types';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import log from '#renderer/global/service/logger';
import { getErrorMessage } from '#renderer/global/service/utils';
import type { ClaudeCheckResult } from '#renderer/domains/onboarding/service/types';

// Step completion checks (cached across popover open/close)
let cachedClaudeCheck: ClaudeCheckResult | null = null;

export function useStepStates() {
  const { data: config } = useConfig();
  const saveMut = useConfigSave();
  const getConfig = useConfigSnapshot();
  const hasBotToken = !!config?.env?.TELEGRAM_MANDO_BOT_TOKEN;
  const [claudeResult, setClaudeResult] = useState<ClaudeCheckResult | null>(cachedClaudeCheck);

  const persistClaudeOk = useCallback(
    (result: ClaudeCheckResult) => {
      if (result.installed && result.works) {
        if (!config?.features?.claudeCodeVerified) {
          const current = getConfig();
          const updated: MandoConfig = {
            ...current,
            features: { ...(current.features || {}), claudeCodeVerified: true },
          };
          saveMut.mutate(updated);
        }
      }
    },
    [config?.features?.claudeCodeVerified, saveMut, getConfig],
  );

  useMountEffect(() => {
    if (cachedClaudeCheck !== null) return;
    window.mandoAPI
      ?.checkClaudeCode?.()
      .then((v) => {
        cachedClaudeCheck = v;
        setClaudeResult(v);
        persistClaudeOk(v);
      })
      .catch((err) => {
        log.error('checkClaudeCode failed:', err);
        const fail: ClaudeCheckResult = {
          installed: false,
          version: null,
          works: false,
          checkFailed: true,
          error: getErrorMessage(err, 'Unknown error'),
        };
        cachedClaudeCheck = fail;
        setClaudeResult(fail);
      });
  });

  const recheckClaude = useCallback(() => {
    cachedClaudeCheck = null;
    setClaudeResult(null);
    window.mandoAPI
      ?.checkClaudeCode?.()
      .then((v) => {
        cachedClaudeCheck = v;
        setClaudeResult(v);
        persistClaudeOk(v);
      })
      .catch((err) => {
        log.error('checkClaudeCode failed:', err);
        const fail: ClaudeCheckResult = {
          installed: false,
          version: null,
          works: false,
          checkFailed: true,
          error: getErrorMessage(err, 'Unknown error'),
        };
        cachedClaudeCheck = fail;
        setClaudeResult(fail);
      });
  }, [persistClaudeOk]);

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
