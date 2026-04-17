import type { SessionEntry, SessionSummary, TaskItem, TimelineEvent } from '#renderer/global/types';

/** Maps timeline event types to a caller label used when the session_id has no row in the session map. */
const CALLER_MAP: Record<string, string> = {
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
};

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
    const sid = ev.data?.session_id as string | undefined;
    if (!sid || seen.has(sid)) continue;
    const existing = sessionMap[sid];
    seen.set(sid, {
      session_id: sid,
      status: existing?.status ?? 'stopped',
      caller: existing?.caller ?? CALLER_MAP[ev.event_type] ?? 'worker',
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

const CALLER_LABELS: Record<string, string> = {
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
};

/** Prefix-to-canonical mapping for callers with embedded IDs. */
const CALLER_PREFIXES: [string, string][] = [
  ['advisor:', 'advisor'],
  ['task-ask:', 'task-ask'],
  ['parse-todos-', 'parse-todos'],
  ['planning-cc-r', 'planning-cc-feedback'],
  ['planning-synth-r', 'planning-synth'],
];

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
      resumed: s.resumed ? 1 : 0,
      task_id: String(taskId),
      worker_name: s.worker_name || '',
      status: s.status,
    })),
  );
}

const CATEGORY_ORDER = [
  'worker',
  'captain-review-async',
  'captain-merge-async',
  'clarifier',
  'deep-clarifier',
  'clarifier-retry',
  'ask',
  'advisor',
  'terminal',
  'triage',
  'nudge',
  'adopt',
];

/** Sorts category keys: known categories in canonical order, unknowns appended. */
export function sortCategories(categories: Record<string, number>): string[] {
  const sorted = CATEGORY_ORDER.filter((c) => c in categories);
  for (const c of Object.keys(categories)) {
    if (!sorted.includes(c)) sorted.push(c);
  }
  return sorted;
}

/** Session status filter options for the sessions list. */
export const SESSION_STATUS_OPTIONS = ['all', 'running', 'stopped', 'failed'] as const;
