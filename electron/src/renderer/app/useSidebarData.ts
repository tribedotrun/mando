import React from 'react';
import type { TaskItem, WorkbenchStatusFilter } from '#renderer/global/types';
import { assembleProjectChildren, type SidebarChild } from '#renderer/global/service/utils';
import { useTaskList, useWorkbenchList } from '#renderer/domains/captain';
import { useConfig } from '#renderer/global/runtime/useConfig';
import type { PinnedEntry } from '#renderer/global/ui/SidebarPinnedSection';

export interface SidebarData {
  pinnedItems: PinnedEntry[];
  projectCounts: Record<string, number>;
  projectChildren: Record<string, SidebarChild[]>;
  projects: string[];
  projectLogos: Record<string, string>;
  scoutEnabled: boolean;
}

export function useSidebarData(workbenchFilter: WorkbenchStatusFilter): SidebarData {
  const { data: taskData } = useTaskList();
  const items = taskData?.items ?? [];
  const { data: activeWorkbenches = [] } = useWorkbenchList();
  const { data: filteredWorkbenches = [] } = useWorkbenchList(workbenchFilter);
  const { data: allWorkbenches = [] } = useWorkbenchList('all');
  const { data: _config } = useConfig();
  const scoutEnabled = !!_config?.features?.scout;
  const configProjects = _config?.captain?.projects;

  const pathToName = React.useMemo(() => {
    const map: Record<string, string> = {};
    if (configProjects) {
      for (const [key, proj] of Object.entries(configProjects)) {
        if (proj.name) {
          map[key] = proj.name;
          if (proj.path && proj.path !== key) map[proj.path] = proj.name;
        }
      }
    }
    return map;
  }, [configProjects]);

  const wbTaskMap = React.useMemo(() => {
    const map = new Map<number, TaskItem>();
    for (const task of items) {
      if (task.workbench_id) map.set(task.workbench_id, task);
    }
    return map;
  }, [items]);

  const pinnedItems: PinnedEntry[] = React.useMemo(() => {
    return activeWorkbenches
      .filter((wb) => wb.pinnedAt && !wb.archivedAt)
      .sort((a, b) => (b.pinnedAt! > a.pinnedAt! ? 1 : b.pinnedAt! < a.pinnedAt! ? -1 : 0))
      .map((wb) => ({
        wb,
        task: wbTaskMap.get(wb.id),
        project: pathToName[wb.project] ?? wb.project,
      }));
  }, [activeWorkbenches, wbTaskMap, pathToName]);

  const pinnedWbIds = React.useMemo(() => new Set(pinnedItems.map((p) => p.wb.id)), [pinnedItems]);
  const allWorkbenchIds = React.useMemo(
    () => new Set(allWorkbenches.map((wb) => wb.id)),
    [allWorkbenches],
  );

  const { projectCounts, projectChildren } = React.useMemo(
    () =>
      assembleProjectChildren({
        tasks: items,
        filteredWorkbenches,
        allWorkbenchIds,
        wbTaskMap,
        pinnedWbIds,
        pathToName,
        workbenchFilter,
      }),
    [
      items,
      filteredWorkbenches,
      allWorkbenchIds,
      wbTaskMap,
      pathToName,
      pinnedWbIds,
      workbenchFilter,
    ],
  );

  const projects = React.useMemo(() => {
    const names = new Set(Object.keys(projectCounts));
    if (configProjects) {
      for (const proj of Object.values(configProjects)) {
        if (proj.name) names.add(proj.name);
      }
    }
    return [...names].sort();
  }, [projectCounts, configProjects]);

  const projectLogos = React.useMemo(() => {
    const map: Record<string, string> = {};
    if (configProjects) {
      for (const proj of Object.values(configProjects)) {
        if (proj.name && proj.logo) map[proj.name] = proj.logo;
      }
    }
    return map;
  }, [configProjects]);

  return {
    pinnedItems,
    projectCounts,
    projectChildren,
    projects,
    projectLogos,
    scoutEnabled,
  };
}
