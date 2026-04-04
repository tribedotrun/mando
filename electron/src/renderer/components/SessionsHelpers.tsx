import React from 'react';
import type { SessionEntry } from '#renderer/types';

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
      <svg width="48" height="48" viewBox="0 0 48 48" fill="none" className="mb-4">
        <rect
          x="6"
          y="10"
          width="36"
          height="28"
          rx="4"
          stroke="var(--color-text-4)"
          strokeWidth="1.5"
        />
        <path
          d="M14 20h8M14 26h5"
          stroke="var(--color-text-4)"
          strokeWidth="1.5"
          strokeLinecap="round"
        />
        <circle cx="35" cy="19" r="2" stroke="var(--color-text-4)" strokeWidth="1.5" />
      </svg>
      <span className="text-subheading" style={{ color: 'var(--color-text-2)' }}>
        No sessions yet
      </span>
    </div>
  );
}
