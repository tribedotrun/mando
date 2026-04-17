import { useMemo } from 'react';
import { useConfig } from '#renderer/global/runtime/useConfig';

/**
 * Resolves a project display name to the set of paths that identify it
 * (display name, config key, and configured path). Returns null when no
 * project filter is active.
 */
export function useProjectFilterPaths(projectFilter?: string | null): Set<string> | null {
  const { data: config } = useConfig();
  const configProjects = config?.captain?.projects;

  return useMemo(() => {
    if (!projectFilter) return null;
    const paths = new Set<string>();
    paths.add(projectFilter);
    if (configProjects) {
      for (const [key, proj] of Object.entries(configProjects)) {
        if (proj.name === projectFilter) {
          paths.add(key);
          if (proj.path) paths.add(proj.path);
        }
      }
    }
    return paths;
  }, [projectFilter, configProjects]);
}
