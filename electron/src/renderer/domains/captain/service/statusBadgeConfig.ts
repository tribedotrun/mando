export interface StatusBadge {
  label: string;
  color: string;
  pulse?: boolean;
}

const CONFIG: Record<string, StatusBadge> = {
  'in-progress': { label: 'Streaming', color: 'var(--success)', pulse: true },
  clarifying: { label: 'Streaming', color: 'var(--success)', pulse: true },
  new: { label: 'Queued', color: 'var(--text-4)' },
  queued: { label: 'Queued', color: 'var(--text-4)' },
  'captain-reviewing': { label: 'Reviewing', color: 'var(--success)', pulse: true },
  'captain-merging': { label: 'Merging', color: 'var(--success)', pulse: true },
  'awaiting-review': { label: 'Ready for review', color: 'var(--review)' },
  escalated: { label: 'Escalated', color: 'var(--destructive)' },
  'needs-clarification': { label: 'Needs input', color: 'var(--needs-human)' },
  errored: { label: 'Failed', color: 'var(--destructive)' },
  rework: { label: 'Rework', color: 'var(--destructive)' },
  'handed-off': { label: 'Handed off', color: 'var(--text-3)' },
  merged: { label: 'Merged', color: 'var(--text-4)' },
  canceled: { label: 'Canceled', color: 'var(--text-4)' },
};

const FALLBACK: StatusBadge = { label: 'Completed', color: 'var(--text-4)' };

/** Returns the badge config for a task status. */
export function getStatusBadge(status: string): StatusBadge {
  return CONFIG[status] ?? FALLBACK;
}

/** Statuses that need the active session duration appended. */
export function isStreamingStatus(status: string): boolean {
  return status === 'in-progress' || status === 'clarifying';
}
