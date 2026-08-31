import { useCallback, useState } from 'react';
import type { NotificationKind } from '#shared/notifications';
import { toast } from '#renderer/global/runtime/useFeedback';
import { getErrorMessage } from '#renderer/global/service/utils';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import {
  openInFinder as openInFinderNative,
  openInCursor as openInCursorNative,
  selectDirectory as selectDirectoryNative,
  openLogsFolder as openLogsFolderNative,
  openConfigFile as openConfigFileNative,
  openDataDir as openDataDirNative,
  openLocalPath as openLocalPathNative,
  toggleDevTools as toggleDevToolsNative,
} from '#renderer/global/providers/native/shell';
import {
  restartDaemon as restartDaemonNative,
  setLoginItem as setLoginItemNative,
  subscribeShortcut,
} from '#renderer/global/providers/native/app';
import { checkClaudeCode as checkClaudeCodeNative } from '#renderer/global/providers/native/onboarding';
import { subscribeNotificationClick } from '#renderer/global/providers/native/notifications';
import {
  subscribeUpdateReady,
  getPendingUpdate,
  installUpdate as installNativeUpdate,
} from '#renderer/global/providers/native/updates';
import log from '#renderer/global/service/logger';

function openPath(fn: () => Promise<void>, label: string): void {
  void (async () => {
    try {
      await fn();
    } catch (err) {
      toast.error(getErrorMessage(err, `Failed to open in ${label}`));
    }
  })();
}

export function useNativeActions() {
  const openInFinder = useCallback((path: string) => {
    openPath(() => openInFinderNative(path), 'Finder');
  }, []);

  const openInCursor = useCallback((path: string) => {
    openPath(() => openInCursorNative(path), 'Cursor');
  }, []);

  const openLocalPath = useCallback((path: string) => {
    openPath(() => openLocalPathNative(path), 'default app');
  }, []);

  // invariant: IPC passthrough; null means user dismissed the dialog (not an error); no failure path to propagate
  const selectDirectory = useCallback(async (): Promise<string | null> => {
    return selectDirectoryNative();
  }, []);

  const openLogsFolder = useCallback(() => {
    openLogsFolderNative();
  }, []);

  const openConfigFile = useCallback(() => {
    openConfigFileNative();
  }, []);

  const openDataDir = useCallback(() => {
    openDataDirNative();
  }, []);

  const toggleDevTools = useCallback(() => {
    void toggleDevToolsNative();
  }, []);

  const restartDaemon = useCallback(() => {
    return restartDaemonNative();
  }, []);

  const setLoginItem = useCallback(async (enabled: boolean) => {
    await setLoginItemNative(enabled);
  }, []);

  const checkClaudeCode = useCallback(async () => {
    return checkClaudeCodeNative();
  }, []);

  return {
    files: {
      openInFinder,
      openInCursor,
      openLocalPath,
      selectDirectory,
      openLogsFolder,
      openConfigFile,
      openDataDir,
    },
    app: { toggleDevTools, restartDaemon },
    system: { setLoginItem, checkClaudeCode },
  };
}

/** Subscribe to IPC shortcut actions from the main process. */
export function useMainShortcuts(onAction: (action: string) => void) {
  useMountEffect(() => {
    return subscribeShortcut(onAction);
  });
}

/** Subscribe to notification clicks from the main process. */
export function useNotificationClicks(
  onData: (data: { item_id?: string; kind: NotificationKind }) => void,
) {
  useMountEffect(() => {
    return subscribeNotificationClick(onData);
  });
}

/** Subscribe to update-ready events and provide install action. */
export function useUpdateBanner() {
  const [updateReady, setUpdateReady] = useState(false);
  const [installing, setInstalling] = useState(false);

  useMountEffect(() => {
    const unsubscribe = subscribeUpdateReady(() => setUpdateReady(true));
    void (async () => {
      try {
        const p = await getPendingUpdate();
        if (p) setUpdateReady(true);
      } catch (err: unknown) {
        log.warn('[useUpdateBanner] getPending check failed:', err);
      }
    })();
    return unsubscribe;
  });

  const installUpdate = useCallback(() => {
    void (async () => {
      setInstalling(true);
      try {
        await installNativeUpdate();
      } catch (err: unknown) {
        log.error('[useUpdateBanner] installUpdate failed:', err);
        toast.error(getErrorMessage(err, 'Update installation failed. Please try again.'));
        setUpdateReady(false);
      } finally {
        setInstalling(false);
      }
    })();
  }, []);

  return { updateReady, installing, installUpdate };
}
