import { useQuery } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { apiGet } from '#renderer/global/providers/http';
import type { MandoConfig } from '#renderer/global/types';

export function useConfig() {
  return useQuery<MandoConfig>({
    queryKey: queryKeys.config.current(),
    queryFn: async (): Promise<MandoConfig> => {
      try {
        return await apiGet<MandoConfig>('/api/config');
      } catch {
        const raw = await window.mandoAPI.readConfig();
        return (typeof raw === 'string' ? JSON.parse(raw) : raw) as MandoConfig;
      }
    },
  });
}
