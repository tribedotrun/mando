import { useCallback } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import type { MandoConfig } from '#renderer/global/types';

export { useConfig } from '#renderer/global/repo/queries';
export {
  useConfigSave,
  useProjectEdit,
  useProjectRemove,
  useProjectAdd,
} from '#renderer/global/repo/configMutations';

export {
  useCredentialsList,
  useCredentialAdd,
  useCredentialRemove,
  useCredentialReveal,
  useCredentialProbe,
  type CredentialInfo,
  type CredentialWindowInfo,
  type CredentialRateLimitStatus,
} from '#renderer/domains/settings/repo/credentials';

/**
 * Returns a stable callback that reads the latest config from the React Query cache.
 * Returns `null` when the cache hasn't loaded the config yet — callers should defend.
 */
export function useConfigSnapshot() {
  const qc = useQueryClient();
  return useCallback(
    (): MandoConfig | null => qc.getQueryData<MandoConfig>(queryKeys.config.current()) ?? null,
    [qc],
  );
}

export { useConfigPatch } from '#renderer/global/runtime/useConfigPatch';
export { useLoginItemToggle } from '#renderer/domains/settings/runtime/useLoginItemToggle';
export {
  useAppVersion,
  useUpdateSystemInfo,
  useTelegramHealth,
  type TelegramHealth,
  type UpdateSystemInfo,
} from '#renderer/domains/settings/repo/queries';

/** Invalidates all config queries. Wraps queryKeys so UI never imports repo. */
export function useConfigInvalidate() {
  const qc = useQueryClient();
  return useCallback(() => {
    void qc.invalidateQueries({ queryKey: queryKeys.config.all });
  }, [qc]);
}
