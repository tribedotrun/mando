import { useMemo } from 'react';
import { useConfig } from '#renderer/global/repo/queries';

/**
 * Returns the sorted list of configured project names. Reads from
 * the React Query config cache (kept fresh by SSE + mutations) so updates
 * propagate immediately on project add/remove/rename.
 */
export function useProjects(): string[] {
  const { data: config } = useConfig();
  const configProjects = config?.captain?.projects;
  return useMemo(() => {
    if (!configProjects) return [];
    const names: string[] = [];
    for (const proj of Object.values(configProjects)) {
      if (proj.name) names.push(proj.name);
    }
    return names.sort();
  }, [configProjects]);
}
