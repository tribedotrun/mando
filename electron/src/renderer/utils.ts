import type { TaskItem } from '#renderer/types';
import { FINALIZED_STATUSES } from '#renderer/types';

/** Extract the last path segment from a GitHub `owner/repo` string. */
export function shortRepo(project?: string): string {
  if (!project) return '\u2014';
  return project.split('/').pop() ?? project;
}

/* ── PR display helpers ── */
const PR_URL_RE = /\/pull\/(\d+)/;

/** Normalize a PR value (full URL or #number) to a short `#N` label. */
export function prLabel(pr: string): string {
  const m = pr.match(PR_URL_RE);
  if (m) return `#${m[1]}`;
  return pr.startsWith('#') ? pr : `#${pr}`;
}

/** Build a GitHub PR href from a PR ref + GitHub repo slug. Always reconstructs
 *  the URL from the slug so stale full URLs (e.g. after a repo rename) are corrected. */
export function prHref(pr: string, githubRepo: string): string {
  const m = pr.match(PR_URL_RE);
  const num = m ? m[1] : pr.replace('#', '');
  return `https://github.com/${githubRepo}/pull/${num}`;
}

/* ── Task status predicates ── */

/** Whether a task can be merged (has PR + project, awaiting review). */
export function canMerge(t: TaskItem): boolean {
  return !!t.pr && !!t.project && t.status === 'awaiting-review';
}

/** Whether a task can be reopened (terminal or review states). */
export function canReopen(t: TaskItem): boolean {
  return ['awaiting-review', 'escalated', 'errored', 'handed-off', 'completed-no-pr'].includes(
    t.status,
  );
}

/** Whether a task can be reworked (fresh worktree + new worker). */
export function canRework(t: TaskItem): boolean {
  return ['awaiting-review', 'handed-off', 'escalated', 'errored'].includes(t.status);
}

/** Whether a task can be asked a question. */
export function canAsk(t: TaskItem): boolean {
  return ['awaiting-review', 'escalated'].includes(t.status);
}

/** Whether a task can be restarted (broader than rework — includes merged/canceled). */
export function canRestart(t: TaskItem): boolean {
  return [
    'awaiting-review',
    'merged',
    'completed-no-pr',
    'canceled',
    'escalated',
    'errored',
  ].includes(t.status);
}

/** Derive PR icon state from task status. */
export function prState(status: string): 'open' | 'merged' | 'closed' {
  if (status === 'merged') return 'merged';
  if (status === 'canceled') return 'closed';
  return 'open';
}

/* ── Task sort ── */

/** Canonical sort: non-finalized first, then descending by last activity. */
export function sortTaskItems(items: TaskItem[]): TaskItem[] {
  return [...items].sort((a, b) => {
    const aFinal = FINALIZED_STATUSES.includes(a.status) ? 1 : 0;
    const bFinal = FINALIZED_STATUSES.includes(b.status) ? 1 : 0;
    if (aFinal !== bFinal) return aFinal - bFinal;
    const ta = a.last_activity_at || a.created_at || '';
    const tb = b.last_activity_at || b.created_at || '';
    return tb.localeCompare(ta);
  });
}

/** Extract a human-readable message from an unknown error. */
export function getErrorMessage(err: unknown, fallback: string): string {
  return err instanceof Error ? err.message : fallback;
}

/** Create a store mutation helper that re-fetches on success and sets error on failure. */
export function createMutate(
  getState: () => { fetch: () => Promise<void> },
  set: (partial: { error: string }) => void,
): (fn: () => Promise<unknown>, errLabel: string) => Promise<void> {
  return async (fn, errLabel) => {
    try {
      await fn();
      await getState().fetch();
    } catch (err) {
      set({ error: getErrorMessage(err, errLabel) });
      throw err;
    }
  };
}

/** Human-readable relative time from an ISO timestamp (e.g. "3m ago", "in 2h"). */
export function relativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  if (Number.isNaN(diff)) return iso.slice(0, 16);
  const abs = Math.abs(diff);
  const future = diff < 0;
  const seconds = Math.floor(abs / 1000);
  if (seconds < 60) return future ? `in ${seconds}s` : seconds < 1 ? 'now' : `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return future ? `in ${minutes}m` : `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return future ? `in ${hours}h` : `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return future ? `in ${days}d` : `${days}d ago`;
}

/** Short timestamp: "Mar 27, 02:45 PM". Returns em-dash for empty/invalid. */
export function shortTs(iso: string): string {
  if (!iso) return '\u2014';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '\u2014';
  return d.toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

/** Human-readable duration from seconds (e.g. "3m 12s"). */
export function fmtDuration(sec: number): string {
  if (sec < 60) return `${Math.round(sec)}s`;
  const m = Math.floor(sec / 60);
  const s = Math.round(sec % 60);
  return s > 0 ? `${m}m ${s}s` : `${m}m`;
}
