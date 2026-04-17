import { useCallback, useRef } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useConfigSave } from '#renderer/global/repo/configMutations';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import type { MandoConfig } from '#renderer/global/types';

type ConfigTransform = (current: MandoConfig) => MandoConfig;

const DEFAULT_DEBOUNCE_MS = 1500;

/**
 * Provides instant and debounced config patching.
 * Both read the latest config from the React Query cache at execution time
 * (not at call time) so debounced saves always use fresh data.
 */
export function useConfigPatch(debounceMs = DEFAULT_DEBOUNCE_MS) {
  const qc = useQueryClient();
  const saveMut = useConfigSave();
  const pendingRef = useRef<ConfigTransform | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  const getConfig = useCallback(
    () => qc.getQueryData<MandoConfig>(queryKeys.config.current()) ?? ({} as MandoConfig),
    [qc],
  );

  const save = useCallback(
    (transform: ConfigTransform, options?: Parameters<typeof saveMut.mutate>[1]) => {
      saveMut.mutate(transform(getConfig()), options);
    },
    [saveMut, getConfig],
  );

  const debouncedSave = useCallback(
    (transform: ConfigTransform) => {
      pendingRef.current = transform;
      if (timerRef.current) clearTimeout(timerRef.current);
      timerRef.current = setTimeout(() => {
        timerRef.current = undefined;
        if (pendingRef.current) {
          saveMut.mutate(pendingRef.current(getConfig()));
          pendingRef.current = null;
        }
      }, debounceMs);
    },
    [saveMut, getConfig, debounceMs],
  );

  return { save, debouncedSave, saveMut };
}
