import React from 'react';
import { Monitor } from 'lucide-react';
import type { SessionEntry, SessionSummary, TaskItem, TimelineEvent } from '#renderer/types';

/** Status → dot color for session list rows. */
const STATUS_COLOR: Record<string, string> = {
  running: 'var(--color-success)',
  stopped: 'var(--color-text-3)',
  failed: 'var(--color-error)',
};

/** Shared dot indicator for session status across SessionsCard + task detail sessions tab. */
export function SessionDot({ status }: { status?: string }): React.ReactElement {
  const color = STATUS_COLOR[status ?? ''] ?? 'var(--color-text-4)';
  return (
    <span
      className={`inline-block h-2 w-2 shrink-0 rounded-full${status === 'running' ? ' animate-pulse' : ''}`}
      style={{ background: color }}
    />
  );
}

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
  return [...seen.values()];
}

/**
 * Build a map of session_id → sequence number for any caller type that has
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
  'captain-review-async': 'captain review',
  'exhaustion-report': 'exhaustion',
  'task-ask': 'ask',
  'scout-process': 'scout',
  'scout-article': 'article',
  'scout-qa': 'Q&A',
  'scout-research': 'research',
  'scout-act': 'act',
  rebase: 'rebase',
};

export function formatCallerLabel(caller: string): string {
  return CALLER_LABELS[caller] ?? caller;
}

/**
 * Primary title — the caller type, capitalized.
 * Sequence number (e.g. "#2") is handled separately in the row renderer.
 */
export function sessionTitle(s: SessionEntry): string {
  const label = formatCallerLabel(s.caller);
  return label.charAt(0).toUpperCase() + label.slice(1);
}

/**
 * Secondary line — contextual detail: task title, scout item title, or date.
 * Returns null when there's nothing useful to show.
 */
export function sessionSubtitle(s: SessionEntry): string | null {
  if (s.task_title) return s.task_title;
  if (s.scout_item_title) return s.scout_item_title;
  return null;
}

export function SessionsEmptyState(): React.ReactElement {
  return (
    <div className="flex flex-col items-center justify-center py-16">
      <Monitor size={48} color="var(--color-text-4)" strokeWidth={1} className="mb-4" />
      <span className="text-subheading text-text-2">No sessions yet</span>
    </div>
  );
}
