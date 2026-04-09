import { useMemo } from 'react';
import { useTaskList, useTaskListWithArchived } from '#renderer/hooks/queries';
import { useTaskFilters } from '#renderer/domains/captain/stores/taskFilters';
import { useProjectFilterPaths } from '#renderer/domains/settings';
import { ACTION_NEEDED_STATUSES, IN_PROGRESS_STATUSES, type TaskItem } from '#renderer/types';
import { sortTaskItems } from '#renderer/utils';

/**
 * Derives a filtered + sorted task list from React Query + filter store.
 *
 * Archive filtering is server-side. The canonical task list (used by SSE
 * and mutations) always excludes archived. When `showArchived` is on, a
 * separate query fetches the full list including archived tasks.
 */
export function useFilteredTasks(projectFilter?: string | null): TaskItem[] {
  const showArchived = useTaskFilters((s) => s.showArchived);
  const normalList = useTaskList();
  const archivedList = useTaskListWithArchived(showArchived);
  const taskData = showArchived ? (archivedList.data ?? normalList.data) : normalList.data;
  const items = taskData?.items ?? [];
  const statusFilter = useTaskFilters((s) => s.statusFilter);
  const filterPaths = useProjectFilterPaths(projectFilter);

  return useMemo(() => {
    let filtered = items;
    if (statusFilter === 'action-needed')
      filtered = items.filter((i) => ACTION_NEEDED_STATUSES.includes(i.status));
    else if (statusFilter === 'in-progress-group')
      filtered = items.filter((i) => IN_PROGRESS_STATUSES.includes(i.status));
    else if (statusFilter) filtered = items.filter((i) => i.status === statusFilter);
    if (filterPaths) filtered = filtered.filter((i) => i.project && filterPaths.has(i.project));
    return sortTaskItems(filtered);
  }, [items, statusFilter, filterPaths]);
}
