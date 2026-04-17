import { useCallback } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import type { TaskListResponse } from '#renderer/global/types';

/** Returns a stable callback that looks up a task's workbench_id from the React Query cache. */
export function useTaskWorkbenchLookup() {
  const qc = useQueryClient();
  return useCallback(
    (taskId: number): number | null => {
      const data = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      const task = data?.items.find((t) => t.id === taskId);
      return task?.workbench_id ?? null;
    },
    [qc],
  );
}
