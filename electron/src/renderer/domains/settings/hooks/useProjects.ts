import { useMemo } from 'react';
import { useSettingsStore } from '#renderer/domains/settings/stores/settingsStore';

/**
 * Returns the sorted list of configured project names. Reads directly from
 * the settings store (kept fresh by load/save) so updates propagate
 * immediately on project add/remove/rename without extra HTTP fetches.
 */
export function useProjects(): string[] {
  const configProjects = useSettingsStore((s) => s.config.captain?.projects);
  return useMemo(() => {
    if (!configProjects) return [];
    const names: string[] = [];
    for (const proj of Object.values(configProjects)) {
      if (proj.name) names.push(proj.name);
    }
    return names.sort();
  }, [configProjects]);
}
