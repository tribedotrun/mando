import { useQuery } from '@tanstack/react-query';
import { apiGet } from '#renderer/global/providers/http';

export interface TelegramHealth {
  enabled: boolean;
  running: boolean;
  owner: string;
  lastError: string | null;
  degraded: boolean;
  restartCount: number;
  mode: string;
}

export function useTelegramHealth() {
  return useQuery<TelegramHealth>({
    queryKey: ['health', 'telegram'],
    queryFn: () => apiGet<TelegramHealth>('/api/health/telegram'),
    refetchInterval: 10_000,
  });
}
