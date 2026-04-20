import { useCallback } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import type { MandoConfig } from '#renderer/global/types';

export { useConfig } from '#renderer/global/repo/queries';
export { useConfigSave, useProjectAdd } from '#renderer/global/repo/configMutations';

/**
 * Returns a stable callback that reads the latest config from the React Query cache.
 * Returns `null` when the cache hasn't loaded the config yet — callers should defend.
 */
export function useConfigSnapshot() {
  const qc = useQueryClient();
  return useCallback((): MandoConfig | null => {
    return qc.getQueryData<MandoConfig>(queryKeys.config.current()) ?? null;
  }, [qc]);
}
