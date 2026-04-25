import { useCallback, useState } from 'react';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import { buildOnboardingConfig } from '#renderer/domains/onboarding/service/buildOnboardingConfig';
import { useConfigSnapshot } from '#renderer/domains/onboarding/runtime/hooks';
import {
  checkClaudeCode as checkClaudeCodeNative,
  saveConfigLocal,
  setupComplete,
  subscribeSetupProgress,
} from '#renderer/global/providers/native/onboarding';
import { selectDirectory as selectDirectoryNative } from '#renderer/global/providers/native/shell';
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
  const getConfigSnapshot = useConfigSnapshot();

  useMountEffect(() => subscribeSetupProgress(setProgressMsg));

  // invariant: errors are encoded in CCResult.checkFailed; no failure path propagates to the caller
  const checkClaudeCode = useCallback(async (): Promise<CCResult> => {
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
  }, []);

  // invariant: IPC passthrough; null means user dismissed the dialog (not an error); no failure path to propagate
  const selectDirectory = useCallback(async (): Promise<string | null> => {
    return selectDirectoryNative();
  }, []);

  const saveProgress = useCallback(
    async (tgToken: string) => {
      const config = buildOnboardingConfig({ tgToken }, getConfigSnapshot());
      await saveConfigLocal(JSON.stringify(config, null, 2));
    },
    [getConfigSnapshot],
  );

  const completeSetup = useCallback(
    async (tgToken: string) => {
      const config = buildOnboardingConfig({ tgToken, autoSchedule: true }, getConfigSnapshot());
      return setupComplete(JSON.stringify(config, null, 2));
    },
    [getConfigSnapshot],
  );

  return { progressMsg, saveProgress, completeSetup, checkClaudeCode, selectDirectory };
}
