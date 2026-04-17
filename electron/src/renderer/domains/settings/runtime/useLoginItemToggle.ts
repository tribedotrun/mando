import { useState, useCallback } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useConfigSave } from '#renderer/global/repo/configMutations';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import type { MandoConfig } from '#renderer/global/types';
import log from '#renderer/global/service/logger';
import { toast } from 'sonner';

/**
 * Encapsulates the login-item toggle saga:
 * 1. Save config with new value
 * 2. On success, call IPC to set the OS login item
 * 3. If IPC fails, revert the config (compensating transaction)
 */
export function useLoginItemToggle(setLoginItem: (enabled: boolean) => Promise<void>) {
  const [saving, setSaving] = useState(false);
  const qc = useQueryClient();
  const saveMut = useConfigSave();

  const getConfig = useCallback(
    () => qc.getQueryData<MandoConfig>(queryKeys.config.current()) ?? ({} as MandoConfig),
    [qc],
  );

  const toggle = useCallback(
    (currentValue: boolean) => {
      setSaving(true);
      const current = getConfig();
      const next = !currentValue;
      const updated: MandoConfig = { ...current, ui: { ...(current.ui || {}), openAtLogin: next } };
      saveMut.mutate(updated, {
        onSuccess: () => {
          void setLoginItem(next).catch((err) => {
            log.error('[Settings] login item IPC failed:', err);
            const reverted: MandoConfig = {
              ...current,
              ui: { ...(current.ui || {}), openAtLogin: !next },
            };
            saveMut.mutate(reverted);
            toast.error('Failed to change login setting');
          });
        },
        onError: () => {
          toast.error('Failed to save login setting');
        },
        onSettled: () => {
          setSaving(false);
        },
      });
    },
    [getConfig, saveMut, setLoginItem],
  );

  return { toggle, saving };
}
