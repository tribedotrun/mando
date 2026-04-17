/** Human-action states get a subtle inline label before the title. */
export const ACTION_LABELS: Record<string, { color: string; label: string }> = {
  'awaiting-review': { color: 'var(--review)', label: 'Review' },
  escalated: { color: 'var(--destructive)', label: 'Escalated' },
  'needs-clarification': { color: 'var(--needs-human)', label: 'Needs input' },
};

/** Human-readable tooltip for each task status. */
export const STATUS_TOOLTIP: Record<string, string> = {
  new: 'Queued',
  queued: 'Queued',
  clarifying: 'Clarifying',
  'in-progress': 'Working',
  'captain-reviewing': 'Reviewing',
  'captain-merging': 'Merging',
  'awaiting-review': 'Awaiting review',
  escalated: 'Escalated',
  'needs-clarification': 'Needs input',
  rework: 'Rework',
  'handed-off': 'Handed off',
  errored: 'Errored',
  merged: 'Merged',
  'completed-no-pr': 'Done',
  'plan-ready': 'Plan ready',
  canceled: 'Canceled',
};
