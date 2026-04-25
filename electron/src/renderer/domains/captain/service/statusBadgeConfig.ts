import type { ItemStatus } from '#renderer/global/types';

export interface StatusBadge {
  label: string;
  color: string;
  pulse?: boolean;
}

const CONFIG: Partial<Record<ItemStatus, StatusBadge>> = Object.freeze({
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
  stopped: { label: 'Stopped', color: 'var(--text-3)' },
});

const FALLBACK: StatusBadge = Object.freeze({ label: 'Completed', color: 'var(--text-4)' });

/** Returns the badge config for a task status. */
export function getStatusBadge(status: ItemStatus): StatusBadge {
  return CONFIG[status] ?? FALLBACK;
}

/** Statuses that need the active session duration appended. */
export function isStreamingStatus(status: ItemStatus): boolean {
  return status === 'in-progress' || status === 'clarifying';
}

/**
 * Resolve the display-time pause state for a task. Returns the pause
 * badge (with a localised reset time) when every credential in the pool
 * is in rate-limit cooldown and `paused_until` is still in the future,
 * else `null` so the caller falls through to the normal status badge.
 *
 * `nowSecondsEpoch` is injected so UI callers stay side-effect free; a
 * hook wires it via `Date.now()` at render time.
 */
export function resolvePausedBadge(
  pausedUntil: number | null,
  nowSecondsEpoch: number,
): StatusBadge | null {
  if (pausedUntil === null || pausedUntil <= nowSecondsEpoch) {
    return null;
  }
  const resetLocal = new Date(pausedUntil * 1000).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });
  return { label: `Paused · resumes ${resetLocal}`, color: '#d97706' };
}
