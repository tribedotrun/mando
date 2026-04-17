import type { TaskListResponse, TaskItem } from '#renderer/global/types';

/** Optimistic setter: map over task items, replacing the matched item. */
export function updateTaskInList(
  old: TaskListResponse | undefined,
  id: number,
  patch: Partial<TaskItem>,
): TaskListResponse | undefined {
  if (!old) return old;
  return {
    ...old,
    items: old.items.map((item) => (item.id === id ? { ...item, ...patch } : item)),
  };
}

/** Optimistic setter: remove tasks by id. */
export function removeTasksFromList(
  old: TaskListResponse | undefined,
  ids: Set<number>,
): TaskListResponse | undefined {
  if (!old) return old;
  const items = old.items.filter((item) => !ids.has(item.id));
  return { ...old, items, count: items.length };
}
