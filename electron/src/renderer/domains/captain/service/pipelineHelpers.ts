import {
  ACTION_NEEDED_STATUSES,
  FINALIZED_STATUSES,
  WORKING_STATUSES,
  type TaskItem,
  type ItemStatus,
} from '#renderer/global/types';

export interface StatCounts {
  queued: number;
  working: number;
  actionNeeded: number;
  errored: number;
}

export function computeCounts(items: TaskItem[]): StatCounts {
  const actionSet = new Set<ItemStatus>(ACTION_NEEDED_STATUSES);
  const finalSet = new Set<ItemStatus>(FINALIZED_STATUSES);
  const workingSet = new Set<ItemStatus>(WORKING_STATUSES);

  let queued = 0;
  let working = 0;
  let actionNeeded = 0;
  let errored = 0;

  for (const t of items) {
    if (finalSet.has(t.status)) continue;

    if (workingSet.has(t.status)) {
      working++;
    } else if (t.status === 'new' || t.status === 'queued') {
      queued++;
    } else if (t.status === 'errored') {
      errored++;
    } else if (actionSet.has(t.status)) {
      actionNeeded++;
    }
  }

  return { queued, working, actionNeeded, errored };
}
