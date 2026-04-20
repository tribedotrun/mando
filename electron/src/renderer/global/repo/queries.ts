import { useQuery } from '@tanstack/react-query';
import { highlight } from '#renderer/global/providers/highlighter';
import { fromWireConfig } from '#renderer/global/repo/configMutations';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { apiGetRouteR } from '#renderer/global/providers/http';
import {
  getAppMode,
  getGatewayUrl,
  getDevGitInfo,
  readConfigFallback,
} from '#renderer/global/providers/native/app';
import { ApiErrorThrown, toReactQuery } from '#result';
import type { MandoConfig } from '#renderer/global/types';
import { parseConfigJsonText } from '#shared/daemon-contract/json';

export interface DevInfo {
  mode: string;
  port: string;
  branch: string;
  commit: string;
  worktree: string | null;
  slot: string | null;
}

/** Load dev info (mode, port, git state) from IPC. Returns null in production. */
export function useDevInfoQuery() {
  return useQuery<DevInfo | null>({
    queryKey: queryKeys.onboarding.appInfo(),
    queryFn: async () => {
      const [mode, gatewayUrl, gitInfo] = await Promise.all([
        getAppMode(),
        getGatewayUrl(),
        getDevGitInfo(),
      ]);
      if (mode === 'production' || mode === 'clean') return null;
      if (!gatewayUrl) return null;
      const port = new URL(gatewayUrl).port;
      return {
        mode: mode.toUpperCase(),
        port,
        branch: gitInfo.branch,
        commit: gitInfo.commit,
        worktree: gitInfo.worktree,
        slot: gitInfo.slot,
      };
    },
    staleTime: Infinity,
    gcTime: Infinity,
  });
}

/** Query hook for syntax-highlighted HTML. Caches aggressively. */
export function useHighlight(code: string, lang: string) {
  return useQuery({
    queryKey: queryKeys.highlighter.code(lang, code),
    queryFn: () => highlight(code, lang),
    staleTime: Infinity,
    gcTime: 5 * 60 * 1000,
  });
}

export function useConfig() {
  return useQuery<MandoConfig>({
    queryKey: queryKeys.config.current(),
    queryFn: async (): Promise<MandoConfig> => {
      try {
        return fromWireConfig(await toReactQuery(apiGetRouteR('getConfig')));
      } catch (e) {
        // invariant: non-network config failures must surface unchanged to the caller
        if (
          !(e instanceof ApiErrorThrown) ||
          (e.apiError.code !== 'network' && e.apiError.code !== 'timeout')
        ) {
          return Promise.reject(e);
        }
        const raw = await readConfigFallback();
        const parsed = parseConfigJsonText(raw, 'query:config-fallback');
        if (parsed.isErr()) {
          return Promise.reject(
            new Error(
              `Config fallback parse failed: ${parsed.error.issues[0]?.message ?? 'unknown'}`,
              { cause: e },
            ),
          );
        }
        return fromWireConfig(parsed.value);
      }
    },
  });
}
