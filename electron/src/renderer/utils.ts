import type { TaskItem, TimelineEvent, ClarifierQuestion, ItemStatus } from '#renderer/types';
import { FINALIZED_STATUSES } from '#renderer/types';
import { useToastStore } from '#renderer/global/stores/toastStore';
import log from '#renderer/logger';

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

/** Whether a task can be asked a question in its terminal/review states (narrow). */
export function canAskTerminal(t: TaskItem): boolean {
  return ['awaiting-review', 'escalated'].includes(t.status);
}

/**
 * Whether a task can be asked a question in any active or review state (broad).
 * Superset of canAskTerminal — also covers in-progress, captain reviews, merging,
 * and clarifying states so the action bar surface lets the human query mid-flight.
 */
export function canAskAny(t: TaskItem): boolean {
  return [
    'awaiting-review',
    'escalated',
    'in-progress',
    'captain-reviewing',
    'captain-merging',
    'clarifying',
  ].includes(t.status);
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

/** Extract the latest clarifier question set from timeline events, if the task is in the `needs-clarification` state. */
export function extractClarifierQuestions(
  events: TimelineEvent[],
  status: ItemStatus,
): ClarifierQuestion[] | null {
  if (status !== 'needs-clarification') return null;
  for (let i = events.length - 1; i >= 0; i--) {
    const e = events[i];
    if (e.event_type !== 'clarify_question') continue;
    const q = e.data?.questions;
    if (Array.isArray(q)) return q as ClarifierQuestion[];
  }
  return null;
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

/** Shorten absolute macOS paths by replacing the home directory with `~`. */
export function shortenPath(path: string): string {
  const m = path.match(/^\/Users\/[^/]+/);
  return m ? '~' + path.slice(m[0].length) : path;
}

/** Human-readable duration from seconds (e.g. "3m 12s"). */
export function fmtDuration(sec: number): string {
  if (sec < 60) return `${Math.round(sec)}s`;
  const m = Math.floor(sec / 60);
  const s = Math.round(sec % 60);
  return s > 0 ? `${m}m ${s}s` : `${m}m`;
}

/**
 * Map a clarification API result status to a toast variant + message.
 * Used by both the detail view clarifier card and useTaskActions.handleAnswer.
 */
export function clarifyResultToToast(status: string | undefined): {
  variant: 'success' | 'info';
  msg: string;
} {
  switch (status) {
    case 'ready':
      return { variant: 'success', msg: 'Clarified, task queued' };
    case 'clarifying':
      return { variant: 'info', msg: 'Still needs more info' };
    case 'escalate':
      return { variant: 'info', msg: 'Escalated to captain review' };
    default:
      return { variant: 'success', msg: 'Answer saved' };
  }
}

/**
 * Write text to the clipboard and show a toast on success or failure.
 * Returns true on success so callers can decide whether to run follow-up logic.
 */
export async function copyToClipboard(text: string, label?: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    if (label) {
      useToastStore.getState().add('success', label);
    }
    return true;
  } catch (err) {
    log.warn('clipboard write failed:', err);
    useToastStore
      .getState()
      .add('error', getErrorMessage(err, 'Copy failed, clipboard access denied'));
    return false;
  }
}

/* ── Numeric helpers (extracted to keep Math.* out of component files) ── */

/** Clamp a value between lo and hi (inclusive). */
export function clamp(v: number, lo: number, hi: number): number {
  return Math.min(Math.max(v, lo), hi);
}

/** Increment index, clamped to [0, maxIndex]. */
export function indexNext(i: number, maxIndex: number): number {
  return Math.min(i + 1, maxIndex);
}

/** Decrement index, clamped to [0, maxIndex]. */
export function indexPrev(i: number): number {
  return Math.max(i - 1, 0);
}

/** Wrap-around modulo (always non-negative). */
export function wrapIndex(i: number, length: number): number {
  const len = Math.max(length, 1);
  return ((i % len) + len) % len;
}

/** Round to nearest integer. */
export function round(v: number): number {
  return Math.round(v);
}

/** Floor to nearest integer. */
export function floor(v: number): number {
  return Math.floor(v);
}

/** Ceiling to nearest integer. */
export function ceil(v: number): number {
  return Math.ceil(v);
}

/** Format a number as USD with 2 decimal places (e.g. "1.50"). */
export function fmtUsd(v: number): string {
  return v.toFixed(2);
}

/** Format milliseconds as a short human duration (e.g. "3m" or "45s"). */
export function fmtMs(ms: number): string {
  return ms >= 60_000 ? `${Math.round(ms / 60_000)}m` : `${Math.round(ms / 1_000)}s`;
}

/** Compute a whole-number percentage (0-100). */
export function pct(completed: number, total: number): number {
  if (total === 0) return 0;
  return Math.round((completed / total) * 100);
}

/** Format worker runtime from an ISO start timestamp (e.g. "3h 12m"). */
export function fmtRuntime(startedAt?: string): string {
  if (!startedAt) return '-';
  const start = new Date(startedAt).getTime();
  if (Number.isNaN(start)) return '-';
  const diffMs = Date.now() - start;
  if (diffMs < 0) return '-';
  const totalMin = Math.floor(diffMs / 60_000);
  const h = Math.floor(totalMin / 60);
  const m = totalMin % 60;
  return h > 0 ? `${h}h ${m}m` : `${m}m`;
}

/** Compute textarea row count for bulk mode. */
export function bulkTextareaRows(lineCount: number): number {
  return Math.max(6, Math.min(12, lineCount));
}

/** Ceil-based minutes remaining (e.g. rate limit countdown). */
export function ceilMinutes(secs: number): number {
  return Math.ceil(secs / 60);
}
