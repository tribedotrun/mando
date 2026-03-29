import { useMemo } from 'react';
import { useTaskStore } from '#renderer/stores/taskStore';
import type { TaskItem } from '#renderer/types';
import { sortTaskItems } from '#renderer/utils';
import { ACTION_NEEDED_STATUSES, IN_PROGRESS_STATUSES } from '#renderer/types';

/**
 * Derives a filtered + sorted task list from stable store slices.
 *
 * Selects items, statusFilter, and showArchived individually so that
 * useSyncExternalStore never sees an unstable snapshot (which would
 * cause an infinite re-render loop in React 19).
 */
export function useFilteredTasks(projectFilter?: string | null): TaskItem[] {
  const items = useTaskStore((s) => s.items);
  const statusFilter = useTaskStore((s) => s.statusFilter);
  const showArchived = useTaskStore((s) => s.showArchived);

  return useMemo(() => {
    let filtered = items;
    if (statusFilter === 'action-needed')
      filtered = items.filter((i) => ACTION_NEEDED_STATUSES.includes(i.status));
    else if (statusFilter === 'in-progress-group')
      filtered = items.filter((i) => IN_PROGRESS_STATUSES.includes(i.status));
    else if (statusFilter) filtered = items.filter((i) => i.status === statusFilter);
    else if (!showArchived) filtered = items.filter((i) => !i.archived_at);
    if (projectFilter) filtered = filtered.filter((i) => i.project === projectFilter);
    return sortTaskItems(filtered);
  }, [items, statusFilter, showArchived, projectFilter]);
}
