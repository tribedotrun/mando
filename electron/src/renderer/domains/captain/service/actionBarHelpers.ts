import { FINALIZED_STATUSES, type AskHistoryEntry, type TaskItem } from '#renderer/global/types';
import { canAskAny, canReopen, canRework } from '#renderer/global/service/utils';

export type ActionBarAction = 'ask' | 'reopen' | 'rework';

export const ACTION_CONFIG: Record<
  ActionBarAction,
  { label: string; placeholder: string; requiresInput: boolean }
> = {
  ask: { label: 'Ask', placeholder: 'Ask about this task...', requiresInput: true },
  reopen: { label: 'Reopen', placeholder: 'Feedback for reopen...', requiresInput: true },
  rework: { label: 'Rework', placeholder: 'Feedback for rework...', requiresInput: true },
};

export function getAvailableActions(item: TaskItem): ActionBarAction[] {
  const actions: ActionBarAction[] = [];
  if (canAskAny(item)) actions.push('ask');
  if (canReopen(item)) actions.push('reopen');
  if (canRework(item)) actions.push('rework');
  return actions;
}

export function getDefaultAction(item: TaskItem): ActionBarAction {
  const available = getAvailableActions(item);
  if (available.includes('ask')) return 'ask';
  return available[0] ?? 'ask';
}

const HIDDEN_STATUSES = Object.freeze([
  'needs-clarification',
  'captain-reviewing',
  'new',
  'queued',
] as const);

/** Whether to show the "ask-reopen" shortcut button. */
export function shouldShowAskReopen(
  item: TaskItem,
  selectedAction: ActionBarAction,
  history?: AskHistoryEntry[],
): boolean {
  if (selectedAction !== 'ask') return false;
  if (item.status !== 'awaiting-review' && item.status !== 'escalated') return false;
  if (!item.session_ids?.ask) return false;
  return !!history?.some((m) => m.role === 'assistant' && !m.content.startsWith('Error: '));
}

/** Whether the action bar should be hidden for the given task. */
export function isActionBarHidden(item: TaskItem): boolean {
  return (
    FINALIZED_STATUSES.includes(item.status) ||
    (HIDDEN_STATUSES as readonly string[]).includes(item.status) ||
    getAvailableActions(item).length === 0
  );
}
