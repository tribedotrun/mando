import { useCallback } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import type { MandoConfig } from '#renderer/global/types';

export { useConfig } from '#renderer/global/runtime/useConfig';
export { useConfigSave, useProjectAdd } from '#renderer/global/repo/configMutations';

/** Returns a stable callback that reads the latest config from the React Query cache. */
export function useConfigSnapshot() {
  const qc = useQueryClient();
  return useCallback(
    () => qc.getQueryData<MandoConfig>(queryKeys.config.current()) ?? ({} as MandoConfig),
    [qc],
  );
}
