import { useCallback, useState } from 'react';
import { toast } from 'sonner';
import { getErrorMessage } from '#renderer/global/service/utils';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';

function openPath(fn: () => Promise<void>, label: string) {
  fn().catch((err) => toast.error(getErrorMessage(err, `Failed to open in ${label}`)));
}

export function useNativeActions() {
  const openInFinder = useCallback((path: string) => {
    openPath(() => window.mandoAPI.openInFinder(path), 'Finder');
  }, []);

  const openInCursor = useCallback((path: string) => {
    openPath(() => window.mandoAPI.openInCursor(path), 'Cursor');
  }, []);

  const selectDirectory = useCallback(async (): Promise<string | null> => {
    return window.mandoAPI.selectDirectory();
  }, []);

  const openLogsFolder = useCallback(() => {
    void window.mandoAPI.openLogsFolder();
  }, []);

  const openConfigFile = useCallback(() => {
    void window.mandoAPI.openConfigFile();
  }, []);

  const openDataDir = useCallback(() => {
    void window.mandoAPI.openDataDir();
  }, []);

  const toggleDevTools = useCallback(() => {
    void window.mandoAPI.toggleDevTools();
  }, []);

  const restartDaemon = useCallback(() => {
    void window.mandoAPI.restartDaemon().finally(() => window.location.reload());
  }, []);

  const setLoginItem = useCallback(async (enabled: boolean) => {
    await window.mandoAPI.setLoginItem(enabled);
  }, []);

  const checkClaudeCode = useCallback(async () => {
    return window.mandoAPI?.checkClaudeCode?.();
  }, []);

  return {
    openInFinder,
    openInCursor,
    selectDirectory,
    openLogsFolder,
    openConfigFile,
    openDataDir,
    toggleDevTools,
    restartDaemon,
    setLoginItem,
    checkClaudeCode,
  };
}

/** Subscribe to IPC shortcut actions from the main process. */
export function useMainShortcuts(onAction: (action: string) => void) {
  useMountEffect(() => {
    if (!window.mandoAPI) return;
    window.mandoAPI.onShortcut(onAction);
    return () => window.mandoAPI.removeShortcutListeners();
  });
}

/** Subscribe to notification clicks from the main process. */
export function useNotificationClicks(onData: (data: { item_id?: string; kind: unknown }) => void) {
  useMountEffect(() => {
    if (!window.mandoAPI) return;
    window.mandoAPI.onNotificationClick(onData);
    return () => window.mandoAPI.removeNotificationClickListeners();
  });
}

/** Subscribe to update-ready events and provide install action. */
export function useUpdateBanner() {
  const [updateReady, setUpdateReady] = useState(false);
  const [installing, setInstalling] = useState(false);

  useMountEffect(() => {
    if (!window.mandoAPI?.updates) return;
    window.mandoAPI.updates.onUpdateReady(() => setUpdateReady(true));
    window.mandoAPI.updates
      .getPending()
      .then((p) => {
        if (p) setUpdateReady(true);
      })
      .catch(() => void 0);
    return () => window.mandoAPI.updates.removeUpdateListeners();
  });

  const installUpdate = useCallback(() => {
    setInstalling(true);
    window.mandoAPI.updates
      .installUpdate()
      .catch(() => setUpdateReady(false))
      .finally(() => setInstalling(false));
  }, []);

  return { updateReady, installing, installUpdate };
}
