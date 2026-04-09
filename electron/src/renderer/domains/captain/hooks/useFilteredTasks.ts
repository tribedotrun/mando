import { useMemo } from 'react';
import { useTaskList } from '#renderer/hooks/queries';
import { useTaskFilters } from '#renderer/domains/captain/stores/taskFilters';
import { useProjectFilterPaths } from '#renderer/domains/settings';
import { ACTION_NEEDED_STATUSES, IN_PROGRESS_STATUSES, type TaskItem } from '#renderer/types';
import { sortTaskItems } from '#renderer/utils';

/**
 * Derives a filtered + sorted task list from React Query + filter store.
 *
 * Reads items from useTaskList (React Query) and filter state from the
 * lightweight useTaskFilters Zustand store.
 */
export function useFilteredTasks(projectFilter?: string | null): TaskItem[] {
  const { data: taskData } = useTaskList();
  const items = taskData?.items ?? [];
  const statusFilter = useTaskFilters((s) => s.statusFilter);
  const showArchived = useTaskFilters((s) => s.showArchived);
  const filterPaths = useProjectFilterPaths(projectFilter);

  return useMemo(() => {
    let filtered = items;
    if (statusFilter === 'action-needed')
      filtered = items.filter((i) => ACTION_NEEDED_STATUSES.includes(i.status));
    else if (statusFilter === 'in-progress-group')
      filtered = items.filter((i) => IN_PROGRESS_STATUSES.includes(i.status));
    else if (statusFilter) filtered = items.filter((i) => i.status === statusFilter);
    else if (!showArchived) filtered = items.filter((i) => !i.archived_at);
    if (filterPaths) filtered = filtered.filter((i) => i.project && filterPaths.has(i.project));
    return sortTaskItems(filtered);
  }, [items, statusFilter, showArchived, filterPaths]);
}
