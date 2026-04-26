import { type ClarifyOutcome, type ItemStatus, type TaskItem } from '#renderer/global/types';

export type { SidebarChild } from '#renderer/global/service/projectChildren';
export {
  sortProjectChildren,
  assembleProjectChildren,
} from '#renderer/global/service/projectChildren';

/** Extract the last path segment from a GitHub `owner/repo` string. */
export function shortRepo(project?: string): string {
  if (!project) return '\u2014';
  return project.split('/').pop() ?? project;
}

/** Format a numeric PR number as a short `#N` label. */
export function prLabel(prNumber: number): string {
  return `#${prNumber}`;
}

/** Build a GitHub PR href from a numeric PR number + GitHub repo slug. */
export function prHref(prNumber: number, githubRepo: string): string {
  return `https://github.com/${githubRepo}/pull/${prNumber}`;
}

/** Whether a task can be merged (has PR + project, awaiting review). */
export function canMerge(task: TaskItem): boolean {
  return !!task.pr_number && !!task.project && task.status === 'awaiting-review';
}

/** Whether a task can be reopened (terminal or review states). */
export function canReopen(task: TaskItem): boolean {
  return [
    'awaiting-review',
    'escalated',
    'errored',
    'handed-off',
    'completed-no-pr',
    'stopped',
  ].includes(task.status);
}

/** Whether a task can be reworked (fresh worktree + new worker). */
export function canRework(task: TaskItem): boolean {
  return ['awaiting-review', 'handed-off', 'escalated', 'errored', 'stopped'].includes(task.status);
}

/** Whether a task can be handed off to a human. */
export function canHandoff(task: TaskItem): boolean {
  return ['awaiting-review', 'escalated'].includes(task.status);
}

/** Whether a task's worker can be stopped (user-initiated halt of in-progress work). */
export function canStop(task: TaskItem): boolean {
  return task.status === 'in-progress';
}

/** Whether a task can be retried after error. */
export function canRetry(task: TaskItem): boolean {
  return task.status === 'errored';
}

/** Whether a task needs clarification answers. */
export function canAnswer(task: TaskItem): boolean {
  return task.status === 'needs-clarification';
}

/** Whether a task's plan can be revised (re-run planning with feedback). */
export function canRevisePlan(task: TaskItem): boolean {
  return task.status === 'plan-ready';
}

/** Whether a task can be asked a question in its terminal/review states (narrow). */
export function canAskTerminal(task: TaskItem): boolean {
  return ['awaiting-review', 'escalated'].includes(task.status);
}

/** Whether a task can be asked a question in any active or review state (broad). */
export function canAskAny(task: TaskItem): boolean {
  return [
    'awaiting-review',
    'escalated',
    'in-progress',
    'captain-reviewing',
    'captain-merging',
    'clarifying',
  ].includes(task.status);
}

/** Derive PR icon state from task status. */
export function prState(status: ItemStatus): 'open' | 'merged' | 'closed' {
  if (status === 'merged') return 'merged';
  if (status === 'canceled') return 'closed';
  return 'open';
}

/** Extract a human-readable message from an unknown error. */
export function getErrorMessage(err: unknown, fallback: string): string {
  return err instanceof Error ? err.message : fallback;
}

/** Convert an ISO timestamp string to local time (e.g. "02:45:12 PM"). */
export function localizeTimestamp(ts: string): string {
  const date = new Date(ts);
  if (Number.isNaN(date.getTime())) return ts;
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

/** Replace all ISO timestamp patterns in arbitrary text with localized times. */
export function localizeMeta(text: string): string {
  return text.replace(
    /\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})?/g,
    (match) => localizeTimestamp(match),
  );
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

/** Format a duration in milliseconds as "Xm Ys" or "Xs". */
export function formatElapsed(ms: number): string {
  const totalSecs = Math.floor(ms / 1000);
  if (totalSecs < 60) return `${totalSecs}s`;
  const mins = Math.floor(totalSecs / 60);
  const secs = totalSecs % 60;
  return `${mins}m ${secs}s`;
}

/** Compact relative time without "ago" suffix (e.g. "4d", "1mo", "2h"). */
export function compactRelativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  if (Number.isNaN(diff)) return '';
  const abs = Math.abs(diff);
  const seconds = Math.floor(abs / 1000);
  if (seconds < 60) return 'now';
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d`;
  const months = Math.floor(days / 30);
  if (months < 12) return `${months}mo`;
  const years = Math.floor(days / 365);
  return `${years}y`;
}

/** Shorten absolute macOS paths by replacing the home directory with `~`. */
export function shortenPath(path: string): string {
  const match = path.match(/^\/Users\/[^/]+/);
  return match ? '~' + path.slice(match[0].length) : path;
}

/** Current wall-clock time in whole unix seconds. */
export function nowEpochSeconds(): number {
  return Math.floor(Date.now() / 1000);
}

/** Human-readable duration from seconds (e.g. "3m 12s"). */
export function fmtDuration(sec: number): string {
  if (sec < 60) return `${Math.round(sec)}s`;
  const minutes = Math.floor(sec / 60);
  const seconds = Math.round(sec % 60);
  return seconds > 0 ? `${minutes}m ${seconds}s` : `${minutes}m`;
}

/** Map a clarification API result status to a toast variant + message.
 *
 * Renderer always submits with `wait=false`, so the daemon acks
 * `'clarifying'` immediately and runs the CC reclarify call in the
 * background. The `'ready' | 'escalate' | 'answered'` branches are not
 * reachable from `useTaskClarify` today — they remain to keep the toast
 * function exhaustive over `ClarifyOutcome`, in case a future caller
 * opts back into the synchronous response.
 */
export function clarifyResultToToast(status: ClarifyOutcome | undefined): {
  variant: 'success' | 'info';
  msg: string;
} {
  switch (status) {
    case 'clarifying':
      return { variant: 'success', msg: 'Answer submitted' };
    case 'ready':
      return { variant: 'success', msg: 'Clarified, task queued' };
    case 'answered':
      return { variant: 'success', msg: 'Clarifier answered your task' };
    case 'escalate':
      return { variant: 'info', msg: 'Escalated to captain review' };
    default:
      return { variant: 'success', msg: 'Answer saved' };
  }
}

/** Clamp a value between lo and hi (inclusive). */
export function clamp(value: number, lo: number, hi: number): number {
  return Math.min(Math.max(value, lo), hi);
}

/** Increment index, clamped to [0, maxIndex]. */
export function indexNext(index: number, maxIndex: number): number {
  return Math.min(index + 1, maxIndex);
}

/** Decrement index, clamped to [0, maxIndex]. */
export function indexPrev(index: number): number {
  return Math.max(index - 1, 0);
}

/** Round to nearest integer. */
export function round(value: number): number {
  return Math.round(value);
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
  const hours = Math.floor(totalMin / 60);
  const minutes = totalMin % 60;
  return hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m`;
}

/** Compute textarea row count for bulk mode. */
export function bulkTextareaRows(lineCount: number): number {
  return Math.max(6, Math.min(12, lineCount));
}

/** Ceil-based minutes remaining (e.g. rate limit countdown). */
export function ceilMinutes(secs: number): number {
  return Math.ceil(secs / 60);
}

const RATE_LIMITED_STATUSES: readonly ItemStatus[] = Object.freeze([
  'captain-reviewing',
  'captain-merging',
  'clarifying',
]);

/** True when global rate-limit cooldown is active and this task is in a blocked status. */
export function isRateLimited(item: { status: ItemStatus }, rateLimitSecs: number): boolean {
  return rateLimitSecs > 0 && RATE_LIMITED_STATUSES.includes(item.status);
}
