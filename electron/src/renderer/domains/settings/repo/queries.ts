import { useQuery } from '@tanstack/react-query';
import { apiGetRouteR } from '#renderer/global/providers/http';
import {
  getAppInfo,
  setLoginItem as setNativeLoginItem,
} from '#renderer/global/providers/native/app';
import { getUpdateAppVersion, getUpdateChannel } from '#renderer/global/providers/native/updates';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { toReactQuery } from '#result';

export interface TelegramHealth {
  enabled: boolean;
  running: boolean;
  owner: string;
  lastError: string | null;
  degraded: boolean;
  restartCount: number;
  mode: string;
}

export type UpdateSystemInfo = {
  appVersion: string;
  channel: Awaited<ReturnType<typeof getUpdateChannel>>;
};

export function useTelegramHealth() {
  return useQuery<TelegramHealth>({
    queryKey: queryKeys.health.telegram(),
    queryFn: () => toReactQuery(apiGetRouteR('getHealthTelegram')),
    refetchInterval: 10_000,
  });
}

export function useAppVersion() {
  return useQuery({
    queryKey: queryKeys.settings.aboutAppVersion(),
    queryFn: async () => {
      const info = await getAppInfo();
      return info.appVersion;
    },
  });
}

export function useUpdateSystemInfo() {
  return useQuery<UpdateSystemInfo>({
    queryKey: queryKeys.settings.generalSystemInfo(),
    queryFn: async () => {
      const [appVersion, channel] = await Promise.all([getUpdateAppVersion(), getUpdateChannel()]);
      return { appVersion, channel };
    },
  });
}

export function setLoginItem(enabled: boolean): Promise<void> {
  return setNativeLoginItem(enabled);
}
