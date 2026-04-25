import React from 'react';
import { useRouterState } from '@tanstack/react-router';
import type { TaskItem } from '#renderer/global/types';
import { useWorkbenchList } from '#renderer/domains/captain/runtime/hooks';
import { useTaskForWorkbench } from '#renderer/domains/captain/runtime/useTaskForWorkbench';

export interface WorkbenchCtx {
  worktreeName: string | null;
  worktreePath: string | null;
  projectName: string | null;
  task: TaskItem | null;
}

/** Resolve the current workbench context from route state. */
export function useWorkbenchCtx(): WorkbenchCtx | null {
  const pathname = useRouterState({ select: (s) => s.location.pathname });

  const wbMatch = pathname.match(/^\/wb\/(\d+)/);
  const wbId = wbMatch ? Number(wbMatch[1]) : null;
  // Use active list (Tier 1, zero refetch) as primary source.
  // Only fetch 'all' when the active query has loaded and the workbench isn't found.
  const { data: activeWbs = [], isLoading: activeLoading } = useWorkbenchList();
  const activeMatch = wbId ? (activeWbs.find((w) => w.id === wbId) ?? null) : null;
  const { data: allWbs = [] } = useWorkbenchList(
    wbId && !activeMatch && !activeLoading ? 'all' : undefined,
  );
  const workbench = activeMatch ?? (wbId ? (allWbs.find((w) => w.id === wbId) ?? null) : null);

  const { task } = useTaskForWorkbench(wbId, workbench);

  return React.useMemo<WorkbenchCtx | null>(() => {
    if (!workbench) return null;

    const wtPath = workbench.worktree;
    return {
      worktreeName: workbench.title ?? wtPath?.split('/').pop() ?? null,
      worktreePath: wtPath,
      projectName: workbench.project ?? null,
      task,
    };
  }, [workbench, task]);
}
