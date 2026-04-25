import { useCallback, useRef, useState } from 'react';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import { setLoginItem, useUpdateSystemInfo } from '#renderer/domains/settings/repo/queries';
import {
  subscribeUpdateChecking,
  subscribeUpdateNoUpdate,
  subscribeUpdateCheckError,
  subscribeUpdateCheckDone,
  checkForUpdates as triggerUpdateCheck,
  installUpdate as triggerUpdateInstall,
  setUpdateChannel,
} from '#renderer/global/providers/native/updates';
import log from '#renderer/global/service/logger';
import { toast } from '#renderer/global/runtime/useFeedback';

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

  const { data: systemInfo } = useUpdateSystemInfo();

  useMountEffect(() => {
    const disposers: Array<() => void> = [];
    disposers.push(
      subscribeUpdateChecking(() => {
        clearTimeout(clearTimerRef.current);
        setUpdateCheckStatus('checking');
      }),
    );
    disposers.push(
      subscribeUpdateNoUpdate(() => {
        setUpdateCheckStatus('up-to-date');
        clearTimerRef.current = setTimeout(() => setUpdateCheckStatus('idle'), STATUS_CLEAR_MS);
      }),
    );
    disposers.push(
      subscribeUpdateCheckError(() => {
        setUpdateCheckStatus('error');
        clearTimerRef.current = setTimeout(() => setUpdateCheckStatus('idle'), STATUS_CLEAR_MS);
      }),
    );
    disposers.push(
      subscribeUpdateCheckDone(({ found }) => {
        if (found) setUpdateCheckStatus('update-available');
      }),
    );
    return () => {
      clearTimeout(clearTimerRef.current);
      for (const dispose of disposers) dispose();
    };
  });

  const appVersion = systemInfo?.appVersion ?? '';
  const updateChannel = channelOverride ?? systemInfo?.channel ?? 'stable';

  const changeChannel = useCallback((channel: string) => {
    if (channel !== 'stable' && channel !== 'beta') return;
    void (async () => {
      setSavingChannel(true);
      setChannelOverride(channel);
      try {
        await setUpdateChannel(channel);
      } catch (err: unknown) {
        log.error('[SettingsGeneral] channel change failed:', err);
        setChannelOverride(null);
        toast.error('Failed to change update channel');
      } finally {
        setSavingChannel(false);
      }
    })();
  }, []);

  const checkForUpdates = useCallback(() => {
    void (async () => {
      try {
        await triggerUpdateCheck();
      } catch (err: unknown) {
        log.error('[useUpdateLifecycle] checkForUpdates failed:', err);
        setUpdateCheckStatus('error');
      }
    })();
  }, []);

  const installUpdate = useCallback(() => {
    return triggerUpdateInstall();
  }, []);

  const onInstallError = useCallback(() => {
    setUpdateCheckStatus('install-error');
    clearTimerRef.current = setTimeout(() => setUpdateCheckStatus('idle'), STATUS_CLEAR_MS);
  }, []);

  return {
    app: { version: appVersion },
    channel: { value: updateChannel, saving: savingChannel, change: changeChannel },
    update: { status: updateCheckStatus, check: checkForUpdates, install: installUpdate },
    login: { setLoginItem },
    events: { onInstallError },
  };
}
