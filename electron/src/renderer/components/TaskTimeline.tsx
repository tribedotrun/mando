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
};

const DOT_COLORS: Record<string, string> = {
  created: 'var(--color-text-4)',
  worker_completed: 'var(--color-success)',
  merged: 'var(--color-success)',
  human_answered: 'var(--color-needs-human)',
  human_reopen: 'var(--color-needs-human)',
  human_ask: 'var(--color-needs-human)',
  clarify_question: 'var(--color-needs-human)',
  escalated: 'var(--color-error)',
  errored: 'var(--color-error)',
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

  const DOT_SIZE = 8;
  const RAIL_LEFT = DOT_SIZE / 2; // center of dot in container coords

  return (
    <div className="relative" style={{ paddingLeft: 20 }}>
      {/* Vertical line — from first dot center to last dot center */}
      {events.length > 1 && (
        <div
          className="absolute w-px"
          style={{
            left: RAIL_LEFT,
            top: 10, // approximate first dot center
            bottom: 14, // approximate last dot center
            background: 'var(--color-border-subtle)',
          }}
        />
      )}
      {events.map((event, i) => {
        const icon = EVENT_ICONS[event.event_type] ?? '\u00B7';
        const dotColor = DOT_COLORS[event.event_type] ?? 'var(--color-border)';
        const sessionId = event.data.session_id as string | undefined;
        const showTranscript = sessionId && !shownSessionIds.has(sessionId);
        if (sessionId) shownSessionIds.add(sessionId);
        const isExpanded = expandedIdx === i;
        const isHollow = dotColor === 'var(--color-text-4)';
        const hasVisibleData =
          event.data &&
          Object.entries(event.data).some(([k]) => k !== 'session_id' && k !== 'source');

        return (
          <div key={`${event.timestamp}-${i}`} className="relative mb-2">
            {/* Dot — centered on the rail */}
            <div
              className="absolute rounded-full"
              style={{
                width: DOT_SIZE,
                height: DOT_SIZE,
                left: -20 + RAIL_LEFT - DOT_SIZE / 2,
                top: 6,
                border: isHollow ? `2px solid ${dotColor}` : 'none',
                background: isHollow ? 'transparent' : dotColor,
              }}
            />
            <div
              className={`flex items-center gap-2 rounded px-1 py-0.5${hasVisibleData ? ' cursor-pointer' : ''}`}
              onClick={hasVisibleData ? () => setExpandedIdx(isExpanded ? null : i) : undefined}
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
                  className="shrink-0"
                  title="View transcript"
                  style={{
                    color: 'var(--color-accent)',
                    background: 'none',
                    border: 'none',
                    cursor: 'pointer',
                    padding: 2,
                    display: 'flex',
                    alignItems: 'center',
                  }}
                >
                  <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
                    <path d="M1.75 1h8.5c.966 0 1.75.784 1.75 1.75v5.5A1.75 1.75 0 0 1 10.25 10H7.061l-2.574 2.573A1.458 1.458 0 0 1 2 11.543V10h-.25A1.75 1.75 0 0 1 0 8.25v-5.5C0 1.784.784 1 1.75 1ZM1.5 2.75v5.5c0 .138.112.25.25.25h1a.75.75 0 0 1 .75.75v2.19l2.72-2.72a.749.749 0 0 1 .53-.22h3.5a.25.25 0 0 0 .25-.25v-5.5a.25.25 0 0 0-.25-.25h-8.5a.25.25 0 0 0-.25.25Zm13 2a.25.25 0 0 0-.25-.25h-.5a.75.75 0 0 1 0-1.5h.5c.966 0 1.75.784 1.75 1.75v5.5A1.75 1.75 0 0 1 14.25 12H14v1.543a1.458 1.458 0 0 1-2.487 1.03L9.22 12.28a.749.749 0 0 1 .326-1.275.749.749 0 0 1 .734.215l2.22 2.22v-2.19a.75.75 0 0 1 .75-.75h1a.25.25 0 0 0 .25-.25Z" />
                  </svg>
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
