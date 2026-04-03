import { useQuery } from '@tanstack/react-query';
import { fetchHealth } from '#renderer/api';

export function useLinearSlug(): string | undefined {
  const { data } = useQuery({
    queryKey: ['status-linear-slug'],
    queryFn: () => fetchHealth(),
    retry: 2,
    retryDelay: 5000,
    select: (s) => s.linear_workspace_slug,
  });
  return data;
}
