import { type TaskItem, type WorkbenchItem } from '#renderer/global/types';

/** Sidebar child: a workbench with optional task metadata. */
export interface SidebarChild {
  wb: WorkbenchItem;
  task?: TaskItem;
}

/** Sort sidebar children descending by last activity (task activity preferred when present). */
export function sortProjectChildren(items: SidebarChild[]): SidebarChild[] {
  const activity = (c: SidebarChild): string =>
    c.task?.last_activity_at || c.task?.created_at || c.wb.lastActivityAt || c.wb.createdAt || '';
  return [...items].sort((a, b) => activity(b).localeCompare(activity(a)));
}

/** Build per-project sidebar child lists using a workbench-first model.
 *  Every visible row is a workbench; tasks ride along as optional metadata.
 *  Orphan tasks are only synthesized when the task has no real workbench --
 *  tasks whose workbench exists but was excluded by the current filter stay
 *  hidden so the filter doesn't leak across states.
 *
 *  `projectCounts` is filter-independent: it counts ALL non-pinned tasks per
 *  project. The delete-project confirmation dialog uses this count to gate a
 *  destructive action that deletes every task server-side, so it must reflect
 *  the true task total regardless of what's currently visible. */
export function assembleProjectChildren(opts: {
  tasks: TaskItem[];
  filteredWorkbenches: WorkbenchItem[];
  allWorkbenchIds: Set<number>;
  wbTaskMap: Map<number, TaskItem>;
  pinnedWbIds: Set<number>;
  pathToName: Record<string, string>;
  workbenchFilter: 'active' | 'archived' | 'all';
}): {
  projectCounts: Record<string, number>;
  projectChildren: Record<string, SidebarChild[]>;
} {
  const {
    tasks,
    filteredWorkbenches,
    allWorkbenchIds,
    wbTaskMap,
    pinnedWbIds,
    pathToName,
    workbenchFilter,
  } = opts;
  const counts: Record<string, number> = {};
  const children: Record<string, SidebarChild[]> = {};
  const seenTaskIds = new Set<number>();

  for (const task of tasks) {
    if (!task.project) continue;
    if (task.workbench_id && pinnedWbIds.has(task.workbench_id)) continue;
    const pName = pathToName[task.project] ?? task.project;
    counts[pName] = (counts[pName] || 0) + 1;
  }

  for (const wb of filteredWorkbenches) {
    if (pinnedWbIds.has(wb.id)) continue;
    if (workbenchFilter === 'active' && wb.archivedAt) continue;
    if (workbenchFilter === 'archived' && !wb.archivedAt) continue;
    const task = wbTaskMap.get(wb.id);
    if (task) seenTaskIds.add(task.id);
    const pName = pathToName[wb.project] ?? wb.project;
    counts[pName] ??= 0;
    (children[pName] ??= []).push({ wb, task });
  }

  for (const task of tasks) {
    if (!task.project || seenTaskIds.has(task.id)) continue;
    if (task.workbench_id && allWorkbenchIds.has(task.workbench_id)) continue;
    if (task.workbench_id && pinnedWbIds.has(task.workbench_id)) continue;
    if (workbenchFilter === 'archived') continue;
    const pName = pathToName[task.project] ?? task.project;
    const syntheticWb: WorkbenchItem = {
      id: task.workbench_id ?? 0,
      rev: 0,
      projectId: task.project_id ?? 0,
      project: task.project,
      worktree: task.worktree ?? '',
      title: task.title || task.original_prompt || 'Untitled task',
      createdAt: task.created_at ?? new Date().toISOString(),
      lastActivityAt: task.last_activity_at ?? task.created_at ?? new Date().toISOString(),
      pinnedAt: null,
      archivedAt: null,
      deletedAt: null,
    };
    (children[pName] ??= []).push({ wb: syntheticWb, task });
  }

  for (const [key, arr] of Object.entries(children)) {
    children[key] = sortProjectChildren(arr);
  }
  return { projectCounts: counts, projectChildren: children };
}
