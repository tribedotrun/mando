import type {
  SessionCategory,
  SessionEntry,
  SessionStatus,
  SessionSummary,
  TaskItem,
  TimelineEvent,
} from '#renderer/global/types';
import { sessionCategorySchema, sessionStatusSchema } from '#shared/daemon-contract/schemas';
import { ApiErrorThrown } from '#result';

/** Maps timeline event types to a caller label used when the session_id has no row in the session map. */
const CALLER_MAP: Record<string, string> = Object.freeze({
  worker_spawned: 'worker',
  worker_completed: 'worker',
  worker_nudged: 'worker',
  session_resumed: 'worker',
  captain_review_started: 'captain-review-async',
  captain_review_verdict: 'captain-review-async',
  clarify_started: 'clarifier',
  clarify_resolved: 'clarifier',
  clarify_question: 'clarifier',
  human_ask: 'task-ask',
  rebase_triggered: 'rebase',
});

/**
 * Build SessionSummary entries from timeline events as the authoritative source.
 * Fills in known sessions from `sessionMap`; invents minimal placeholders for
 * sessions that only appear in the timeline so the detail view never shows a
 * stale subset.
 */
export function buildSessionsFromTimeline(
  events: TimelineEvent[],
  sessionMap: Record<string, SessionSummary>,
  item: TaskItem,
): SessionSummary[] {
  const seen = new Map<string, SessionSummary>();
  for (const ev of events) {
    // Narrow on the payload's event_type tag — only variants that actually
    // carry a session_id contribute to the merged list.
    const payload = ev.data;
    const sid = 'session_id' in payload ? payload.session_id : undefined;
    if (!sid || seen.has(sid)) continue;
    const existing = sessionMap[sid];
    seen.set(sid, {
      session_id: sid,
      status: existing?.status ?? 'stopped',
      caller: existing?.caller ?? CALLER_MAP[payload.event_type] ?? 'worker',
      started_at: existing?.started_at ?? ev.timestamp,
      duration_ms: existing?.duration_ms,
      cost_usd: existing?.cost_usd,
      model: existing?.model,
      resumed: existing?.resumed ?? false,
      cwd: existing?.cwd ?? item.worktree,
      worker_name: existing?.worker_name,
    });
  }
  // Merge DB-sourced sessions not referenced by any timeline event (e.g.
  // advisor, planning, auto-merge-triage sessions that have task_id set
  // but no timeline event carries their session_id).
  for (const [sid, s] of Object.entries(sessionMap)) {
    if (!seen.has(sid)) {
      seen.set(sid, s);
    }
  }
  // Sort chronologically by started_at so the merged list is ordered.
  return [...seen.values()].sort((a, b) => {
    const ta = a.started_at ?? '';
    const tb = b.started_at ?? '';
    return ta < tb ? -1 : ta > tb ? 1 : 0;
  });
}

/**
 * Build a map of session_id -> sequence number for any caller type that has
 * multiple sessions on the same task. Sequences are per (task_id, caller) pair
 * so "worker #2" and "captain review #1" don't share a counter.
 */
export function buildSessionSequence(sessions: SessionEntry[]): Map<string, number> {
  const result = new Map<string, number>();
  const pairCounts = new Map<string, number>();
  const sidToPairKey = new Map<string, string>();
  const withTask = sessions.filter((s) => s.task_id);
  const chronological = [...withTask].reverse();
  for (const s of chronological) {
    const pairKey = `${s.task_id}\0${s.caller}`;
    const seq = (pairCounts.get(pairKey) ?? 0) + 1;
    pairCounts.set(pairKey, seq);
    result.set(s.session_id, seq);
    sidToPairKey.set(s.session_id, pairKey);
  }
  for (const [sid] of result) {
    if ((pairCounts.get(sidToPairKey.get(sid)!) ?? 0) <= 1) result.delete(sid);
  }
  return result;
}

const CALLER_LABELS: Record<string, string> = Object.freeze({
  worker: 'worker',
  clarifier: 'clarifier',
  'deep-clarifier': 'deep clarifier',
  'clarifier-retry': 'clarifier retry',
  'captain-review-async': 'captain review',
  'captain-merge-async': 'captain merge',
  'exhaustion-report': 'exhaustion',
  'task-ask': 'ask',
  advisor: 'advisor',
  'auto-merge-triage': 'merge triage',
  'planning-planner': 'planner',
  'planning-cc-feedback': 'planning feedback',
  'planning-synth': 'planning synthesis',
  'planning-final': 'planning final',
  'scout-process': 'scout',
  'scout-article': 'article',
  'scout-qa': 'Q&A',
  'scout-research': 'research',
  'scout-act': 'act',
  rebase: 'rebase',
});

/** Prefix-to-canonical mapping for callers with embedded IDs. */
const CALLER_PREFIXES: readonly (readonly [string, string])[] = Object.freeze([
  ['advisor:', 'advisor'],
  ['task-ask:', 'task-ask'],
  ['parse-todos-', 'parse-todos'],
  ['planning-cc-r', 'planning-cc-feedback'],
  ['planning-synth-r', 'planning-synth'],
] as const);

/** Extract skill name from prompt body that starts with "Base directory for this skill: .../name" */
export function detectSkill(content: string): string | null {
  const m = content.match(/^Base directory for this skill: .+\/([^/\s]+)/);
  return m ? m[1] : null;
}

export function formatCallerLabel(caller: string): string {
  let label: string;
  const direct = CALLER_LABELS[caller];
  if (direct) {
    label = direct;
  } else {
    label = caller;
    for (const [prefix, canonical] of CALLER_PREFIXES) {
      if (caller.startsWith(prefix)) {
        label = CALLER_LABELS[canonical] ?? canonical;
        break;
      }
    }
  }
  return label.charAt(0).toUpperCase() + label.slice(1);
}

/**
 * Primary title -- the caller type, capitalized.
 * Sequence number (e.g. "#2") is handled separately in the row renderer.
 */
export function sessionTitle(s: SessionEntry): string {
  return formatCallerLabel(s.caller);
}

/**
 * Secondary line -- contextual detail: task title, scout item title, or date.
 * Returns null when there's nothing useful to show.
 */
export function sessionSubtitle(s: SessionEntry): string | null {
  if (s.task_title) return s.task_title;
  if (s.scout_item_title) return s.scout_item_title;
  return null;
}

/** Builds a `claude -r <id>` command, optionally prefixed with `cd`. */
export function buildResumeCmd(sessionId: string, cwd?: string | null): string {
  return cwd ? `cd "${cwd}" && claude -r ${sessionId}` : `claude -r ${sessionId}`;
}

/** Adapts SessionSummary[] to the shape buildSessionSequence expects. */
export function buildSequenceFromSummaries(
  summaries: SessionSummary[],
  taskId: number,
): Map<string, number> {
  return buildSessionSequence(
    summaries.map((s) => ({
      session_id: s.session_id,
      created_at: s.started_at || '',
      cwd: s.cwd || '',
      model: s.model || '',
      caller: s.caller,
      resumed: s.resumed,
      cost_usd: s.cost_usd,
      duration_ms: s.duration_ms,
      turn_count: null,
      scout_item_id: null,
      task_id: String(taskId),
      worker_name: s.worker_name,
      resumed_at: null,
      status: s.status,
      task_title: null,
      scout_item_title: null,
      github_repo: null,
      pr_number: null,
      worktree: null,
      branch: null,
      resume_cwd: null,
      category: null,
      credential_id: null,
      credential_label: null,
      error: null,
      api_error_status: null,
    })),
  );
}

const CATEGORY_ORDER: readonly SessionCategory[] = sessionCategorySchema.options;

/** Sorts category keys in canonical contract order and ignores unknown keys. */
export function sortCategories(categories: Record<string, number>): SessionCategory[] {
  return CATEGORY_ORDER.filter((category) => category in categories);
}

/** Session status filter options for the sessions list. */
export type SessionStatusFilter = 'all' | SessionStatus;

export const SESSION_STATUS_OPTIONS: readonly SessionStatusFilter[] = [
  'all',
  ...sessionStatusSchema.options,
];

/**
 * True when a thrown transcript-fetch error is a 404 from the daemon — the
 * session finished (or never started) without emitting a transcript file.
 * Drives the inline "no transcript recorded" stub in TranscriptPage so
 * failed clarifier/worker sessions don't show the generic error strip.
 */
export function isTranscriptUnavailable(error: unknown): boolean {
  if (!(error instanceof ApiErrorThrown)) return false;
  const api = error.apiError;
  return api.code === 'http' && api.status === 404;
}
