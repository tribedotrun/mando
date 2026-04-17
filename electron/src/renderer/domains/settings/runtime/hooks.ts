import { useCallback } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import type { MandoConfig } from '#renderer/global/types';

export { useConfig } from '#renderer/global/runtime/useConfig';
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
  type CredentialInfo,
} from '#renderer/domains/settings/repo/credentials';

/** Returns a stable callback that reads the latest config from the React Query cache. */
export function useConfigSnapshot() {
  const qc = useQueryClient();
  return useCallback(
    () => qc.getQueryData<MandoConfig>(queryKeys.config.current()) ?? ({} as MandoConfig),
    [qc],
  );
}

export { useConfigPatch } from '#renderer/global/runtime/useConfigPatch';
export { useLoginItemToggle } from '#renderer/domains/settings/runtime/useLoginItemToggle';

/** Invalidates all config queries. Wraps queryKeys so UI never imports repo. */
export function useConfigInvalidate() {
  const qc = useQueryClient();
  return useCallback(() => {
    void qc.invalidateQueries({ queryKey: queryKeys.config.all });
  }, [qc]);
}
