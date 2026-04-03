import React, { useState } from 'react';
import type { TimelineEvent } from '#renderer/types';
import { shortTs } from '#renderer/utils';

const EVENT_ICONS: Record<string, string> = {
  created: '+',
  clarify_started: '\u{1F504}',
  clarify_question: '?',
  clarify_resolved: '\u2713',
  human_answered: '\u{1F4AC}',
  worker_spawned: '\u25B6',
  worker_nudged: '\u2192',
  session_resumed: '\u21BB',
  worker_completed: '\u2713',
  captain_review_started: '\u{1F50D}',
  captain_review_verdict: '\u{1F4CB}',
  awaiting_review: '\u{1F440}',
  human_reopen: '\u21A9',
  human_ask: '\u{1F4AC}',
  rebase_triggered: '\u26A1',
  rework_requested: '\u{1F527}',
  merged: '\u2713\u2713',
  escalated: '\u{1F6A8}',
  errored: '\u2717\u2717',
  canceled: '\u2014',
  handed_off: '\u{1F932}',
  status_changed: '\u{1F500}',
  rate_limited: '\u23F3',
};

const DOT_COLORS: Record<string, string> = {
  created: 'var(--color-text-4)',
  worker_spawned: 'var(--color-accent)',
  worker_completed: 'var(--color-success)',
  merged: 'var(--color-success)',
  human_answered: 'var(--color-needs-human)',
  human_reopen: 'var(--color-needs-human)',
  human_ask: 'var(--color-needs-human)',
  clarify_question: 'var(--color-needs-human)',
  escalated: 'var(--color-error)',
  errored: 'var(--color-error)',
  rate_limited: 'var(--color-text-4)',
};

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
      <div className="text-[12px]" style={{ color: 'var(--color-text-3)' }}>
        No timeline events
      </div>
    );
  }

  return (
    <div className="relative pl-5">
      {/* Vertical line */}
      <div
        className="absolute left-[4px] top-1 bottom-1 w-px"
        style={{ background: 'var(--color-border-subtle)' }}
      />
      {events.map((event, i) => {
        const icon = EVENT_ICONS[event.event_type] ?? '\u00B7';
        const dotColor = DOT_COLORS[event.event_type] ?? 'var(--color-border)';
        const sessionId = event.data.session_id as string | undefined;
        const showTranscript = sessionId && !shownSessionIds.has(sessionId);
        if (sessionId) shownSessionIds.add(sessionId);
        const isExpanded = expandedIdx === i;
        const hasVisibleData =
          event.data && Object.keys(event.data).some((k) => k !== 'session_id' && k !== 'source');

        return (
          <div key={`${event.timestamp}-${i}`} className="relative mb-2">
            {/* Dot */}
            <div
              className="absolute -left-5 top-[5px] h-[10px] w-[10px] rounded-full border-2"
              style={{
                borderColor: dotColor,
                background: dotColor === 'var(--color-text-4)' ? 'transparent' : dotColor,
              }}
            />
            <div
              className={`flex items-center gap-2 rounded px-1 py-0.5${hasVisibleData ? ' cursor-pointer' : ''}`}
              onClick={() => hasVisibleData && setExpandedIdx(isExpanded ? null : i)}
            >
              <span className="w-4 shrink-0 text-center text-[11px]">{icon}</span>
              <span className="text-[12px] font-medium" style={{ color: 'var(--color-text-1)' }}>
                {event.event_type.replace(/_/g, ' ')}
              </span>
              <span
                className="min-w-0 flex-1 truncate text-[11px]"
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
                  className="shrink-0 cursor-pointer rounded border-none bg-transparent px-1.5 py-0.5 text-[10px] font-mono text-[var(--color-accent)] hover:bg-[var(--color-accent-wash)]"
                >
                  transcript
                </button>
              )}
              <span className="shrink-0 text-[10px]" style={{ color: 'var(--color-text-4)' }}>
                {shortTs(event.timestamp)}
              </span>
            </div>
            {isExpanded && hasVisibleData && (
              <div
                className="ml-6 mt-1 rounded border px-2 py-1 text-[11px]"
                style={{
                  background: 'var(--color-surface-2)',
                  borderColor: 'var(--color-border-subtle)',
                  color: 'var(--color-text-2)',
                }}
              >
                {Object.entries(event.data)
                  .filter(([k]) => k !== 'session_id' && k !== 'source')
                  .map(([k, v]) => (
                    <div key={k}>
                      <span style={{ color: 'var(--color-text-4)' }}>{k}:</span> {String(v)}
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
