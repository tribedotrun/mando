import { useCallback } from 'react';
import { useNavigate } from '@tanstack/react-router';
import { useQueryClient } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import log from '#renderer/global/service/logger';
import type { WorkbenchItem } from '#renderer/global/types';

export function useSidebarNav() {
  const navigate = useNavigate();
  const qc = useQueryClient();

  const navigateToWorkbench = useCallback(
    (wbId: number, tab?: string) => {
      void navigate({
        to: '/wb/$workbenchId',
        params: { workbenchId: String(wbId) },
        search: tab ? { tab } : {},
      });
    },
    [navigate],
  );

  const openTaskWorkbench = useCallback(
    (taskId: number, workbenchId?: number) => {
      if (workbenchId) {
        navigateToWorkbench(workbenchId);
        return;
      }
      const task = qc
        .getQueryData<{
          items: Array<{ id: number; workbench_id?: number }>;
        }>(queryKeys.tasks.list())
        ?.items.find((t) => t.id === taskId);
      if (task?.workbench_id) {
        navigateToWorkbench(task.workbench_id);
      } else {
        log.warn('openTaskWorkbench: no workbench resolved', { taskId, inCache: !!task });
      }
    },
    [qc, navigateToWorkbench],
  );

  const openWorktreeWorkbench = useCallback(
    (workbenchId?: number, cwd?: string) => {
      if (workbenchId) {
        navigateToWorkbench(workbenchId, 'terminal');
        return;
      }
      if (cwd) {
        const entries = qc.getQueriesData<WorkbenchItem[]>({
          queryKey: queryKeys.workbenches.all,
        });
        for (const [, list] of entries) {
          const wb = list?.find((w) => w.worktree === cwd);
          if (wb) {
            navigateToWorkbench(wb.id, 'terminal');
            return;
          }
        }
      }
    },
    [qc, navigateToWorkbench],
  );

  return {
    navigate,
    navigateToWorkbench,
    openTaskWorkbench,
    openWorktreeWorkbench,
  };
}
