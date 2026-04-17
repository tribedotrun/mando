import {
  FINALIZED_STATUSES,
  type TaskItem,
  type TimelineEvent,
  type ClarifierQuestion,
  type ItemStatus,
  type WorkbenchItem,
} from '#renderer/global/types';
import { toast } from 'sonner';
import log from '#renderer/global/service/logger';

/** Extract the last path segment from a GitHub `owner/repo` string. */
export function shortRepo(project?: string): string {
  if (!project) return '\u2014';
  return project.split('/').pop() ?? project;
}

/* ── PR display helpers ── */

/** Format a numeric PR number as a short `#N` label. */
export function prLabel(prNumber: number): string {
  return `#${prNumber}`;
}

/** Build a GitHub PR href from a numeric PR number + GitHub repo slug. */
export function prHref(prNumber: number, githubRepo: string): string {
  return `https://github.com/${githubRepo}/pull/${prNumber}`;
}

/* ── Task status predicates ── */

/** Whether a task can be merged (has PR + project, awaiting review). */
export function canMerge(t: TaskItem): boolean {
  return !!t.pr_number && !!t.project && t.status === 'awaiting-review';
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

/** Whether a task can be handed off to a human. */
export function canHandoff(t: TaskItem): boolean {
  return ['awaiting-review', 'escalated'].includes(t.status);
}

/** Whether a task can be retried after error. */
export function canRetry(t: TaskItem): boolean {
  return t.status === 'errored';
}

/** Whether a task needs clarification answers. */
export function canAnswer(t: TaskItem): boolean {
  return t.status === 'needs-clarification';
}

/** Whether a task's plan can be revised (re-run planning with feedback). */
export function canRevisePlan(t: TaskItem): boolean {
  return t.status === 'plan-ready';
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

/** Sidebar child: a workbench with optional task metadata. */
export interface SidebarChild {
  wb: WorkbenchItem;
  task?: TaskItem;
}

/** Sort sidebar children descending by last activity (task activity preferred when present). */
export function sortProjectChildren(items: SidebarChild[]): SidebarChild[] {
  const activity = (c: SidebarChild): string =>
    c.task?.last_activity_at || c.task?.created_at || c.wb.lastActivityAt || c.wb.createdAt || '';
  return [...items].sort((a, b) => activity(b).localeCompare(activity(a)));
}

/** Build per-project sidebar child lists using a workbench-first model.
 *  Every visible row is a workbench; tasks ride along as optional metadata.
 *  Orphan tasks are only synthesized when the task has no real workbench --
 *  tasks whose workbench exists but was excluded by the current filter stay
 *  hidden so the filter doesn't leak across states.
 *
 *  `projectCounts` is filter-independent: it counts ALL non-pinned tasks per
 *  project. The delete-project confirmation dialog uses this count to gate a
 *  destructive action that deletes every task server-side, so it must reflect
 *  the true task total regardless of what's currently visible. */
export function assembleProjectChildren(opts: {
  tasks: TaskItem[];
  filteredWorkbenches: WorkbenchItem[];
  allWorkbenchIds: Set<number>;
  wbTaskMap: Map<number, TaskItem>;
  pinnedWbIds: Set<number>;
  pathToName: Record<string, string>;
  workbenchFilter: 'active' | 'archived' | 'all';
}): {
  projectCounts: Record<string, number>;
  projectChildren: Record<string, SidebarChild[]>;
} {
  const {
    tasks,
    filteredWorkbenches,
    allWorkbenchIds,
    wbTaskMap,
    pinnedWbIds,
    pathToName,
    workbenchFilter,
  } = opts;
  const counts: Record<string, number> = {};
  const children: Record<string, SidebarChild[]> = {};
  const seenTaskIds = new Set<number>();

  // Filter-independent task counts -- used by the delete-project dialog which
  // gates a server-side wipe of all tasks regardless of filter.
  for (const task of tasks) {
    if (!task.project) continue;
    if (task.workbench_id && pinnedWbIds.has(task.workbench_id)) continue;
    const pName = pathToName[task.project] ?? task.project;
    counts[pName] = (counts[pName] || 0) + 1;
  }

  for (const wb of filteredWorkbenches) {
    if (pinnedWbIds.has(wb.id)) continue;
    if (workbenchFilter === 'active' && wb.archivedAt) continue;
    if (workbenchFilter === 'archived' && !wb.archivedAt) continue;
    const task = wbTaskMap.get(wb.id);
    if (task) seenTaskIds.add(task.id);
    const pName = pathToName[wb.project] ?? wb.project;
    counts[pName] ??= 0;
    (children[pName] ??= []).push({ wb, task });
  }

  // Only synthesize rows for truly orphan tasks (no workbench at all). Tasks
  // whose workbench exists but falls outside the current filter stay hidden.
  for (const task of tasks) {
    if (!task.project || seenTaskIds.has(task.id)) continue;
    if (task.workbench_id && allWorkbenchIds.has(task.workbench_id)) continue;
    if (task.workbench_id && pinnedWbIds.has(task.workbench_id)) continue;
    if (workbenchFilter === 'archived') continue;
    const pName = pathToName[task.project] ?? task.project;
    const syntheticWb: WorkbenchItem = {
      id: task.workbench_id ?? 0,
      rev: 0,
      project: task.project,
      worktree: task.worktree ?? '',
      title: task.title || task.original_prompt || 'Untitled task',
      createdAt: task.created_at ?? new Date().toISOString(),
      lastActivityAt: task.last_activity_at ?? task.created_at ?? new Date().toISOString(),
    };
    (children[pName] ??= []).push({ wb: syntheticWb, task });
  }

  for (const [key, arr] of Object.entries(children)) {
    children[key] = sortProjectChildren(arr);
  }
  return { projectCounts: counts, projectChildren: children };
}

/** Extract a human-readable message from an unknown error. */
export function getErrorMessage(err: unknown, fallback: string): string {
  return err instanceof Error ? err.message : fallback;
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

/** Convert an ISO timestamp string to local time (e.g. "02:45:12 PM"). */
export function localizeTimestamp(ts: string): string {
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return ts;
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

/** Replace all ISO timestamp patterns in arbitrary text with localized times. */
export function localizeMeta(text: string): string {
  return text.replace(/\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})?/g, (m) =>
    localizeTimestamp(m),
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
      toast.success(label);
    }
    return true;
  } catch (err) {
    log.warn('clipboard write failed:', err);
    toast.error(getErrorMessage(err, 'Copy failed, clipboard access denied'));
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

/** Round to nearest integer. */
export function round(v: number): number {
  return Math.round(v);
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

const RATE_LIMITED_STATUSES: ReadonlySet<string> = new Set([
  'captain-reviewing',
  'captain-merging',
  'clarifying',
]);

/** True when global rate-limit cooldown is active and this task is in a
 *  status where session spawning is suppressed. */
export function isRateLimited(item: { status: string }, rateLimitSecs: number): boolean {
  return rateLimitSecs > 0 && RATE_LIMITED_STATUSES.has(item.status);
}
