import { useQuery } from '@tanstack/react-query';
import { highlight } from '#renderer/global/runtime/highlighter';

/** Query hook for syntax-highlighted HTML. Caches aggressively. */
export function useHighlight(code: string, lang: string) {
  return useQuery({
    queryKey: ['shiki-highlight', lang, code],
    queryFn: () => highlight(code, lang),
    staleTime: Infinity,
    gcTime: 5 * 60 * 1000,
  });
}
