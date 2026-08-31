import { FINALIZED_STATUSES, type ItemStatus, type TaskItem } from '#renderer/global/types';
import { canReopen, canRework } from '#renderer/global/service/utils';

export type ActionBarAction = 'reopen' | 'rework';

export const ACTION_CONFIG: Record<
  ActionBarAction,
  { label: string; placeholder: string; requiresInput: boolean }
> = {
  reopen: { label: 'Reopen', placeholder: 'Feedback for reopen...', requiresInput: true },
  rework: { label: 'Rework', placeholder: 'Feedback for rework...', requiresInput: true },
};

export function getAvailableActions(item: TaskItem): ActionBarAction[] {
  const actions: ActionBarAction[] = [];
  if (canReopen(item)) actions.push('reopen');
  if (canRework(item)) actions.push('rework');
  return actions;
}

export function getDefaultAction(item: TaskItem): ActionBarAction {
  const available = getAvailableActions(item);
  return available[0] ?? 'reopen';
}

const HIDDEN_STATUSES: readonly ItemStatus[] = Object.freeze([
  'needs-clarification',
  'captain-reviewing',
  'new',
  'queued',
]);

/** Whether the action bar should be hidden for the given task. */
export function isActionBarHidden(item: TaskItem): boolean {
  return (
    FINALIZED_STATUSES.includes(item.status) ||
    HIDDEN_STATUSES.includes(item.status) ||
    getAvailableActions(item).length === 0
  );
}
