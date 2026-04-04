import React, { useState } from 'react';
import type { TimelineEvent } from '#renderer/types';
import { relativeTime } from '#renderer/utils';
import { StatusIcon } from '#renderer/components/TaskActions';
import { PrIcon } from '#renderer/components/TaskIcons';

/** Map timeline event types to StatusIcon status strings. */
const EVENT_ICON_MAP: Record<string, string> = {
  created: 'new',
  worker_spawned: 'in-progress',
  worker_completed: 'completed-no-pr',
  worker_nudged: 'in-progress',
  session_resumed: 'in-progress',
  captain_review_started: 'captain-reviewing',
  captain_review_verdict: 'captain-reviewing',
  captain_merge_started: 'captain-merging',
  awaiting_review: 'awaiting-review',
  escalated: 'escalated',
  errored: 'errored',
  canceled: 'canceled',
  human_reopen: 'needs-clarification',
  human_ask: 'needs-clarification',
  human_answered: 'needs-clarification',
  rework_requested: 'rework',
  handed_off: 'handed-off',
  clarify_started: 'clarifying',
  clarify_question: 'needs-clarification',
  clarify_resolved: 'completed-no-pr',
  status_changed: 'queued',
  rebase_triggered: 'in-progress',
  rate_limited: 'queued',
};

/** Keys to always exclude from expanded detail view. */
const HIDDEN_KEYS = new Set(['session_id', 'source', 'item_id', 'task_id']);

/** Values that add no information — skip them in detail view. */
function isUselessValue(value: unknown): boolean {
  if (value === null || value === undefined || value === '') return true;
  const s = String(value).trim();
  if (s.length === 0) return true;
  // Single-word uppercase status that duplicates the event type
  if (/^[A-Z_]+$/.test(s) && s.length < 40) return true;
  return false;
}

/** Get useful detail entries from event data. */
function getUsefulDetails(data: Record<string, unknown>): [string, string][] {
  return Object.entries(data)
    .filter(([k, v]) => !HIDDEN_KEYS.has(k) && !isUselessValue(v))
    .map(([k, v]) => {
      let display = String(v);
      // Shorten GitHub URLs to repo#number format
      const prMatch = display.match(/github\.com\/([^/]+\/[^/]+)\/pull\/(\d+)/);
      if (prMatch) display = `${prMatch[1]}#${prMatch[2]}`;
      return [k, display] as [string, string];
    });
}

export function TaskTimeline({
  events,
  onTranscriptClick,
}: {
  events: TimelineEvent[];
  onTranscriptClick: (sessionId: string, event: TimelineEvent) => void;
}): React.ReactElement {
  const [expandedIdx, setExpandedIdx] = useState<number | null>(null);
  const shownSessionIds = new Set<string>();

  if (events.length === 0) {
    return (
      <div className="text-caption" style={{ color: 'var(--color-text-3)' }}>
        No timeline events
      </div>
    );
  }

  return (
    <div>
      {events.map((event, i) => {
        const sessionId = event.data.session_id as string | undefined;
        const showTranscript = sessionId && !shownSessionIds.has(sessionId);
        if (sessionId) shownSessionIds.add(sessionId);
        const isExpanded = expandedIdx === i;
        const details = getUsefulDetails(event.data);
        const hasDetails = details.length > 0;
        const isMerged = event.event_type === 'merged';

        return (
          <div key={`${event.timestamp}-${i}`} className="mb-0.5">
            <div
              className={`flex items-center gap-2 rounded-md px-2 py-1 transition-colors${hasDetails ? ' cursor-pointer' : ''} hover:bg-[var(--color-surface-2)]`}
              onClick={() => hasDetails && setExpandedIdx(isExpanded ? null : i)}
            >
              {/* Icon */}
              <span className="flex w-4 shrink-0 items-center justify-center">
                {isMerged ? (
                  <PrIcon state="merged" />
                ) : (
                  <StatusIcon status={EVENT_ICON_MAP[event.event_type] ?? 'queued'} />
                )}
              </span>
              <span className="text-caption font-medium" style={{ color: 'var(--color-text-1)' }}>
                {event.event_type.replace(/_/g, ' ')}
              </span>
              <span
                className="min-w-0 flex-1 truncate text-caption"
                style={{ color: 'var(--color-text-3)' }}
              >
                {event.summary}
              </span>
              {showTranscript && (
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    onTranscriptClick(sessionId, event);
                  }}
                  className="shrink-0 cursor-pointer rounded border-none bg-transparent px-1.5 py-0.5 text-caption text-[var(--color-accent)] hover:bg-[var(--color-accent-wash)]"
                >
                  transcript
                </button>
              )}
              <span className="shrink-0 text-caption" style={{ color: 'var(--color-text-4)' }}>
                {relativeTime(event.timestamp)}
              </span>
            </div>
            {isExpanded && hasDetails && (
              <div
                className="ml-6 mt-1 break-words rounded-md px-3 py-2 text-caption"
                style={{
                  background: 'var(--color-surface-2)',
                  color: 'var(--color-text-2)',
                }}
              >
                {details.map(([k, v]) => (
                  <div key={k} className="mb-0.5">
                    <span style={{ color: 'var(--color-text-4)' }}>{k}:</span> {v}
                  </div>
                ))}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
