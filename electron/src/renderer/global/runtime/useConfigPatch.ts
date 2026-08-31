import { useCallback, useRef } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useConfigSave } from '#renderer/global/repo/configMutations';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { toast } from '#renderer/global/runtime/useFeedback';
import { getErrorMessage } from '#renderer/global/service/utils';
import type { MandoConfig } from '#renderer/global/types';

type ConfigTransform = (current: MandoConfig) => MandoConfig;

const DEFAULT_DEBOUNCE_MS = 1500;

export function useConfigPatch(debounceMs = DEFAULT_DEBOUNCE_MS) {
  const qc = useQueryClient();
  const saveMut = useConfigSave();
  const pendingRef = useRef<ConfigTransform | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  const getConfig = useCallback(
    (): MandoConfig | null => qc.getQueryData<MandoConfig>(queryKeys.config.current()) ?? null,
    [qc],
  );

  const save = useCallback(
    (transform: ConfigTransform, options?: Parameters<typeof saveMut.mutate>[1]) => {
      const current = getConfig();
      if (!current) return;
      saveMut.mutate(transform(current), options);
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
          const current = getConfig();
          if (current) {
            saveMut.mutate(pendingRef.current(current), {
              onError: (err) => {
                toast.error(getErrorMessage(err, 'Failed to save settings'));
              },
            });
          }
          pendingRef.current = null;
        }
      }, debounceMs);
    },
    [saveMut, getConfig, debounceMs],
  );

  return { save, debouncedSave, saveMut };
}
