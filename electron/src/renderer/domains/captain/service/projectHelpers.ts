import type { ProjectConfig } from '#renderer/global/types';

/**
 * Resolves the effective project for task creation forms.
 * If the saved project is not in the project list, falls back.
 * If there is exactly one project, auto-selects it.
 */
export function resolveEffectiveProject(
  selectedProject: string,
  projects: string[],
): { effectiveProject: string; projectRequired: boolean } {
  const saved = selectedProject && projects.includes(selectedProject) ? selectedProject : '';
  return {
    effectiveProject: saved || (projects.length === 1 ? projects[0] : ''),
    projectRequired: projects.length !== 1,
  };
}

/** Returns the copy label for a task plan path based on its type. */
export function planCopyLabel(planPath: string): string {
  return planPath.endsWith('adopt-handoff.md') ? 'Copy handoff path' : 'Copy brief path';
}

/**
 * Resolve a project's stored absolute path from the config-side projects map.
 *
 * Mirrors `settings::resolve_project_config` on the rust side: case-insensitive
 * match on `name`, then `aliases`, then exact key. Returns null when no match
 * or when the matched project lacks a path. The daemon expands `~` on project
 * upsert, so the stored value is always absolute and callers may compare it
 * to other absolute paths via direct string equality.
 */
export function resolveProjectPath(
  projects: Record<string, ProjectConfig> | undefined,
  projectName: string | undefined,
): string | null {
  if (!projects || !projectName) return null;
  const target = projectName.toLowerCase();
  for (const [, proj] of Object.entries(projects)) {
    if (proj.name && proj.name.toLowerCase() === target) {
      return proj.path?.trim() ? proj.path : null;
    }
  }
  for (const [, proj] of Object.entries(projects)) {
    if (proj.aliases?.some((a) => a.toLowerCase() === target)) {
      return proj.path?.trim() ? proj.path : null;
    }
  }
  const direct = projects[projectName];
  if (direct?.path?.trim()) return direct.path;
  return null;
}
