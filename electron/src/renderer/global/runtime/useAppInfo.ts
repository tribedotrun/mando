import { useState } from 'react';
import log from '#renderer/global/service/logger';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';

export interface DevInfo {
  mode: string;
  port: string;
  branch: string;
  commit: string;
  worktree: string | null;
  slot: string | null;
}

let _cachedInfo: DevInfo | null = null;

/** Load dev info (mode, port, git state) from IPC. Caches across remounts. Returns null in production. */
export function useDevInfo(): DevInfo | null {
  const [info, setInfo] = useState<DevInfo | null>(_cachedInfo);

  useMountEffect(() => {
    if (_cachedInfo) return;
    const load = async () => {
      if (!window.mandoAPI) return;
      const [mode, gatewayUrl, gitInfo] = await Promise.all([
        window.mandoAPI.appMode(),
        window.mandoAPI.gatewayUrl(),
        window.mandoAPI.devGitInfo(),
      ]);
      if (mode === 'production' || mode === 'clean') return;
      if (!gatewayUrl) return;
      const port = new URL(gatewayUrl).port;
      const loaded: DevInfo = {
        mode: mode.toUpperCase(),
        port,
        branch: gitInfo.branch,
        commit: gitInfo.commit,
        worktree: gitInfo.worktree,
        slot: gitInfo.slot,
      };
      _cachedInfo = loaded;
      setInfo(loaded);
    };
    load().catch((err) => log.error('[DevInfoBar] failed to load:', err));
  });

  return info;
}
