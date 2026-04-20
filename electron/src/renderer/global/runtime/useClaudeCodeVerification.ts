import { useRef } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import { useNativeActions } from '#renderer/global/runtime/useNativeActions';
import { useConfigSave } from '#renderer/global/repo/configMutations';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import type { MandoConfig } from '#renderer/global/types';
import log from '#renderer/global/service/logger';

const POLL_INTERVAL_MS = 2000;

/**
 * Eagerly checks whether Claude Code is installed and working.
 * Runs once when config is available, polls until config loads.
 * Writes `claudeCodeVerified` to config on success.
 */
export function useClaudeCodeVerification() {
  const qc = useQueryClient();
  const { checkClaudeCode } = useNativeActions();
  const saveMut = useConfigSave();
  const doneRef = useRef(false);

  useMountEffect(() => {
    function getConfig(): MandoConfig | null {
      return qc.getQueryData<MandoConfig>(queryKeys.config.current()) ?? null;
    }

    function tryCheck() {
      if (doneRef.current) return;
      const cfg = getConfig();
      if (!cfg) return;
      if (cfg.features?.claudeCodeVerified || cfg.features?.setupDismissed) {
        doneRef.current = true;
        return;
      }
      doneRef.current = true;
      void checkClaudeCode()
        .then((result) => {
          if (result?.installed && result.works) {
            const current = getConfig();
            if (current && !current.features?.claudeCodeVerified) {
              const updated: MandoConfig = {
                ...current,
                features: { ...(current.features || {}), claudeCodeVerified: true },
              };
              saveMut.mutate(updated, {
                onError: (err) => log.warn('eager CC verification save failed:', err),
              });
            }
          }
        })
        .catch((err) => log.warn('eager CC check failed:', err));
    }

    tryCheck();
    const interval = setInterval(tryCheck, POLL_INTERVAL_MS);
    return () => clearInterval(interval);
  });
}
