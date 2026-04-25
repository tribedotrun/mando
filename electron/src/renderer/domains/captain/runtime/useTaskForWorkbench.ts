import type { TaskItem, WorkbenchItem } from '#renderer/global/types';
import { useTaskList, useTaskListWithArchived } from '#renderer/domains/captain/runtime/hooks';

export interface TaskForWorkbench {
  task: TaskItem | null;
  isLoading: boolean;
}

/**
 * Resolve the task bound to a workbench id with a hidden archived fallback.
 *
 * Mirrors the active → `'all'` pattern that `useWorkbenchPage`/`useWorkbenchCtx`
 * already use for workbenches: try the SSE-patched active list first, then
 * fall back to `useTaskListWithArchived` whenever the active task list missed
 * for a known workbench. The daemon's active task SQL filters tasks via
 * `WHERE w.archived_at IS NULL AND w.deleted_at IS NULL`, and the workbench
 * cache lags the task cache by an SSE patch — relying on
 * `workbench.archivedAt` as a precondition would miss the brief window where
 * the daemon has already dropped the task but the client still sees the
 * workbench as active.
 *
 * `isLoading` stays true while the *active* list is in flight, and while the
 * archived fallback is fetching after an active-list miss. Callers gate on it
 * to avoid rendering a "taskless" branch before the lookup has resolved.
 */
export function useTaskForWorkbench(
  wbId: number | null,
  workbench: WorkbenchItem | null,
): TaskForWorkbench {
  const { data: taskData, isLoading: activeLoading } = useTaskList();
  const activeTask = wbId ? (taskData?.items.find((t) => t.workbench_id === wbId) ?? null) : null;

  // Fall back to the archived list whenever the active lookup resolved with no
  // match for a known workbench, regardless of whether the local
  // `workbench.archivedAt` flag has propagated yet — this closes the transient
  // race where the daemon has already filtered the task out but the workbench
  // SSE patch hasn't arrived. Genuinely taskless workbenches (`+ claude`)
  // pay one cached `?include_archived=true` lookup once the active list
  // resolves; the archived task list is small and React Query memoises the
  // response per session, so the cost is bounded.
  const shouldFetchArchived = Boolean(wbId && workbench && !activeLoading && !activeTask);
  const { data: archivedData, isLoading: archivedLoading } =
    useTaskListWithArchived(shouldFetchArchived);
  const archivedTask =
    wbId && shouldFetchArchived
      ? (archivedData?.items.find((t) => t.workbench_id === wbId) ?? null)
      : null;

  const task = activeTask ?? archivedTask;
  const isLoading = activeLoading || (shouldFetchArchived && archivedLoading);

  return { task, isLoading };
}
