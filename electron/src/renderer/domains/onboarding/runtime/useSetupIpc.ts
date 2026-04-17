import { useCallback, useState } from 'react';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import { buildOnboardingConfig } from '#renderer/domains/onboarding/service/buildOnboardingConfig';
import log from '#renderer/global/service/logger';
import { getErrorMessage } from '#renderer/global/service/utils';

type CCResult = {
  installed: boolean;
  version: string | null;
  works: boolean;
  checkFailed?: boolean;
  error?: string;
} | null;

/** Wraps onboarding IPC calls (setup progress, config save, setup complete, Claude Code check). */
export function useSetupIpc() {
  const [progressMsg, setProgressMsg] = useState<string | null>(null);

  useMountEffect(() => {
    window.mandoAPI.onSetupProgress(setProgressMsg);
  });

  const checkClaudeCode = useCallback(async (): Promise<CCResult> => {
    try {
      return await window.mandoAPI.checkClaudeCode();
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
  }, []);

  const selectDirectory = useCallback(async (): Promise<string | null> => {
    return window.mandoAPI.selectDirectory();
  }, []);

  const saveProgress = useCallback(async (tgToken: string) => {
    const config = buildOnboardingConfig({ tgToken });
    await window.mandoAPI.saveConfigLocal(JSON.stringify(config, null, 2));
  }, []);

  const completeSetup = useCallback(async (tgToken: string) => {
    const config = buildOnboardingConfig({ tgToken, autoSchedule: true });
    return window.mandoAPI.setupComplete(JSON.stringify(config, null, 2));
  }, []);

  return { progressMsg, saveProgress, completeSetup, checkClaudeCode, selectDirectory };
}
