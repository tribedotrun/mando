import { useCallback, useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import log from '#renderer/global/service/logger';
import { toast } from 'sonner';

/** Available update channels. */
export const UPDATE_CHANNELS = ['stable', 'beta'] as const;

export type UpdateCheckStatus =
  | 'idle'
  | 'checking'
  | 'up-to-date'
  | 'update-available'
  | 'error'
  | 'install-error';

const STATUS_CLEAR_MS = 4000;

/** Bundles system info query, update-check lifecycle, and channel switching for the General settings page. */
export function useUpdateLifecycle() {
  const [channelOverride, setChannelOverride] = useState<string | null>(null);
  const [updateCheckStatus, setUpdateCheckStatus] = useState<UpdateCheckStatus>('idle');
  const [savingChannel, setSavingChannel] = useState(false);
  const clearTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  const { data: systemInfo } = useQuery({
    queryKey: ['settings', 'general', 'systemInfo'],
    queryFn: async () => {
      if (!window.mandoAPI) return { appVersion: '', channel: 'stable' };
      const [appVersion, channel] = await Promise.all([
        window.mandoAPI.updates.appVersion(),
        window.mandoAPI.updates.getChannel(),
      ]);
      return { appVersion, channel };
    },
  });

  useMountEffect(() => {
    if (!window.mandoAPI) return;
    window.mandoAPI.updates.onUpdateChecking(() => {
      clearTimeout(clearTimerRef.current);
      setUpdateCheckStatus('checking');
    });
    window.mandoAPI.updates.onUpdateNoUpdate(() => {
      setUpdateCheckStatus('up-to-date');
      clearTimerRef.current = setTimeout(() => setUpdateCheckStatus('idle'), STATUS_CLEAR_MS);
    });
    window.mandoAPI.updates.onUpdateCheckError(() => {
      setUpdateCheckStatus('error');
      clearTimerRef.current = setTimeout(() => setUpdateCheckStatus('idle'), STATUS_CLEAR_MS);
    });
    window.mandoAPI.updates.onUpdateCheckDone(({ found }) => {
      if (found) setUpdateCheckStatus('update-available');
    });
    return () => {
      clearTimeout(clearTimerRef.current);
      window.mandoAPI.updates.removeCheckListeners();
    };
  });

  const appVersion = systemInfo?.appVersion ?? '';
  const updateChannel = channelOverride ?? systemInfo?.channel ?? 'stable';

  const changeChannel = useCallback((channel: string) => {
    setSavingChannel(true);
    setChannelOverride(channel);
    void window.mandoAPI.updates
      .setChannel(channel)
      .catch((err: unknown) => {
        log.error('[SettingsGeneral] channel change failed:', err);
        setChannelOverride(null);
        toast.error('Failed to change update channel');
      })
      .finally(() => setSavingChannel(false));
  }, []);

  const checkForUpdates = useCallback(() => {
    window.mandoAPI.updates.checkForUpdates().catch(() => setUpdateCheckStatus('error'));
  }, []);

  const installUpdate = useCallback(() => {
    return window.mandoAPI.updates.installUpdate();
  }, []);

  const setLoginItem = useCallback(async (enabled: boolean) => {
    await window.mandoAPI.setLoginItem(enabled);
  }, []);

  const onInstallError = useCallback(() => {
    setUpdateCheckStatus('install-error');
    clearTimerRef.current = setTimeout(() => setUpdateCheckStatus('idle'), STATUS_CLEAR_MS);
  }, []);

  return {
    appVersion,
    updateChannel,
    updateCheckStatus,
    savingChannel,
    changeChannel,
    checkForUpdates,
    installUpdate,
    setLoginItem,
    onInstallError,
  };
}
