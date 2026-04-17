import { useCallback, useState } from 'react';
import { useNavigate } from '@tanstack/react-router';
import { useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { createWorktree } from '#renderer/domains/captain';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { getErrorMessage } from '#renderer/global/service/utils';
import log from '#renderer/global/service/logger';
import type { WorkbenchItem } from '#renderer/global/types';

function formatWorktreeSuffix(): string {
  const now = new Date();
  return [
    String(now.getMonth() + 1).padStart(2, '0'),
    String(now.getDate()).padStart(2, '0'),
    '-',
    String(now.getHours()).padStart(2, '0'),
    String(now.getMinutes()).padStart(2, '0'),
    String(now.getSeconds()).padStart(2, '0'),
  ].join('');
}

export function useSidebarNav() {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const [preparingProject, setPreparingProject] = useState<string | null>(null);

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

  const handleNewTerminal = useCallback(
    async (project: string) => {
      if (preparingProject) return;
      setPreparingProject(project);
      try {
        const suffix = formatWorktreeSuffix();
        const result = await createWorktree(project, suffix);
        void qc.invalidateQueries({ queryKey: queryKeys.workbenches.all });
        if (result.workbenchId) {
          navigateToWorkbench(result.workbenchId, 'terminal');
        }
      } catch (err) {
        log.error('createWorktree failed', err);
        toast.error(getErrorMessage(err, 'Failed to create workspace'));
      } finally {
        setPreparingProject(null);
      }
    },
    [preparingProject, qc, navigateToWorkbench],
  );

  return {
    navigate,
    navigateToWorkbench,
    openTaskWorkbench,
    openWorktreeWorkbench,
    handleNewTerminal,
  };
}
