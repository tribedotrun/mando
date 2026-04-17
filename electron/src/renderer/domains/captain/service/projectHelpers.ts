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
