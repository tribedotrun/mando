import React, { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { fetchTimeline, fetchItemSessions, fetchTranscript, fetchHealth } from '#renderer/api';
import type { TaskItem, SessionEntry, SessionSummary, TimelineEvent } from '#renderer/types';
import { SessionDetailPanel } from '#renderer/components/SessionDetailPanel';
import { getErrorMessage, shortTs } from '#renderer/utils';

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

interface Props {
  item: TaskItem;
}

export function SessionsPanel({ item }: Props): React.ReactElement | null {
  const [expandedIdx, setExpandedIdx] = useState<number | null>(null);

  // Session detail panel state (for transcript drill-down)
  const [selectedSession, setSelectedSession] = useState<{
    entry: SessionEntry;
    markdown: string | null;
    loading: boolean;
    error: string | null;
  } | null>(null);

  const { data: linearSlug } = useQuery({
    queryKey: ['status-linear-slug'],
    queryFn: () => fetchHealth(),
    retry: 2,
    retryDelay: 5000,
    select: (s) => s.linear_workspace_slug,
  });

  const { data: timelineData, error: timelineError } = useQuery({
    queryKey: ['sessions-panel', item.id],
    queryFn: async () => {
      const [timeline, sessions] = await Promise.all([
        fetchTimeline(item.id),
        fetchItemSessions(item.id),
      ]);
      const map: Record<string, SessionSummary> = {};
      for (const s of sessions.sessions) {
        map[s.session_id] = s;
      }
      return { events: timeline.events, sessionMap: map };
    },
  });

  const events = timelineData?.events ?? null;
  const sessionMap = timelineData?.sessionMap ?? {};
  const err = timelineError ? getErrorMessage(timelineError, 'Failed to load') : null;

  const handleSessionClick = async (sessionId: string, event: TimelineEvent) => {
    const worker = (event.data.worker as string) || '';
    const summary = sessionMap[sessionId];
    const cwd = summary?.cwd || item.worktree || '';
    const stub: SessionEntry = {
      session_id: sessionId,
      created_at: summary?.started_at || event.timestamp,
      cwd,
      model: '',
      caller: summary?.caller || (event.event_type.startsWith('clarify') ? 'clarifier' : 'worker'),
      resumed: summary?.resumed ? 1 : 0,
      task_id: String(item.id),
      worker_name: worker,
      status: summary?.status || '',
    };
    setSelectedSession({ entry: stub, markdown: null, loading: true, error: null });

    try {
      const data = await fetchTranscript(sessionId);
      setSelectedSession((prev) =>
        prev ? { ...prev, markdown: data.markdown, loading: false } : null,
      );
    } catch (err) {
      const message = getErrorMessage(err, 'Failed to load transcript');
      setSelectedSession((prev) => (prev ? { ...prev, loading: false, error: message } : null));
    }
  };

  if (err) {
    return (
      <div>
        <div
          className="px-6 py-1"
          style={{ background: 'color-mix(in srgb, var(--color-surface-2) 40%, transparent)' }}
        >
          <span className="text-xs" style={{ color: 'var(--color-error)' }}>
            Timeline error: {err}
          </span>
        </div>
      </div>
    );
  }

  if (events === null) {
    return (
      <div>
        <div
          className="px-6 py-1"
          style={{ background: 'color-mix(in srgb, var(--color-surface-2) 40%, transparent)' }}
        >
          <span className="text-xs" style={{ color: 'var(--color-text-3)' }}>
            Loading timeline&hellip;
          </span>
        </div>
      </div>
    );
  }

  if (events.length === 0) return null;

  // Keys to hide from expanded event data (internal metadata or duplicates of summary)
  const HIDDEN_DATA_KEYS = new Set(['session_id', 'source']);

  // Track which session_ids already have a transcript link to avoid duplicates (#5)
  const shownSessionIds = new Set<string>();

  return (
    <>
      <div>
        <div
          className="px-6 py-2"
          style={{ background: 'color-mix(in srgb, var(--color-surface-2) 40%, transparent)' }}
        >
          <div className="text-xs">
            <span className="font-medium" style={{ color: 'var(--color-text-3)' }}>
              timeline ({events.length}):
            </span>
            <div className="mt-1 space-y-px">
              {events.map((event, i) => (
                <TimelineRow
                  key={`${event.timestamp}-${i}`}
                  event={event}
                  index={i}
                  expandedIdx={expandedIdx}
                  setExpandedIdx={setExpandedIdx}
                  shownSessionIds={shownSessionIds}
                  hiddenDataKeys={HIDDEN_DATA_KEYS}
                  onSessionClick={handleSessionClick}
                />
              ))}
            </div>
          </div>
        </div>
      </div>

      {selectedSession && (
        <div>
          <div className="p-0">
            <div className="fixed inset-0 z-[300]">
              <SessionDetailPanel
                session={selectedSession.entry}
                markdown={selectedSession.markdown}
                loading={selectedSession.loading}
                error={selectedSession.error}
                onClose={() => setSelectedSession(null)}
                linearSlug={linearSlug}
                resumeCmd={(() => {
                  const dir = selectedSession.entry.resume_cwd || selectedSession.entry.cwd;
                  return dir
                    ? `cd ${dir} && claude --resume ${selectedSession.entry.session_id}`
                    : `claude --resume ${selectedSession.entry.session_id}`;
                })()}
              />
            </div>
          </div>
        </div>
      )}
    </>
  );
}

/* Extracted to keep the main component under 500 lines */
function TimelineRow({
  event,
  index,
  expandedIdx,
  setExpandedIdx,
  shownSessionIds,
  hiddenDataKeys,
  onSessionClick,
}: {
  event: TimelineEvent;
  index: number;
  expandedIdx: number | null;
  setExpandedIdx: (idx: number | null) => void;
  shownSessionIds: Set<string>;
  hiddenDataKeys: Set<string>;
  onSessionClick: (sessionId: string, event: TimelineEvent) => void;
}): React.ReactElement {
  const icon = EVENT_ICONS[event.event_type] ?? '\u00B7';
  const sessionId = event.data.session_id as string | undefined;
  const content = (event.data.content ?? event.data.feedback) as string | undefined;

  // Only show transcript link for first occurrence of each session_id
  const showTranscript = sessionId && !shownSessionIds.has(sessionId);
  if (sessionId) shownSessionIds.add(sessionId);

  // Filter out hidden keys and keys shown separately (content/feedback)
  const dataEntries = Object.entries(event.data).filter(
    ([k]) => !hiddenDataKeys.has(k) && k !== 'content' && k !== 'feedback',
  );
  const hasData = dataEntries.length > 0;
  const isExpanded = expandedIdx === index;

  return (
    <div>
      <div
        className="flex items-center gap-2 py-0.5 rounded px-1 cursor-pointer"
        onClick={() => setExpandedIdx(isExpanded ? null : index)}
      >
        <span className="w-4 text-center flex-shrink-0">{icon}</span>
        <span className="font-medium truncate" style={{ color: 'var(--color-text-1)' }}>
          {event.event_type.replace(/_/g, ' ')}
        </span>
        <span className="truncate flex-1 min-w-0" style={{ color: 'var(--color-text-3)' }}>
          {event.summary}
        </span>
        {showTranscript && (
          <button
            onClick={(e) => {
              e.stopPropagation();
              onSessionClick(sessionId, event);
            }}
            className="text-[0.6rem] font-mono shrink-0"
            style={{ color: 'var(--color-accent)' }}
            title="View session transcript"
          >
            transcript
          </button>
        )}
        <span
          className="text-[0.65rem] flex-shrink-0 ml-auto"
          style={{ color: 'var(--color-text-4)' }}
        >
          {shortTs(event.timestamp)}
        </span>
      </div>
      {isExpanded && (content || hasData) && (
        <div
          className="ml-6 mb-1 px-2 py-1 rounded border text-[0.65rem]"
          style={{
            background: 'color-mix(in srgb, var(--color-surface-2) 60%, transparent)',
            borderColor: 'color-mix(in srgb, var(--color-border) 20%, transparent)',
            color: 'var(--color-text-2)',
          }}
        >
          {content && (
            <div
              className="whitespace-pre-wrap break-words"
              style={{ color: 'var(--color-text-1)' }}
            >
              {String(content)}
            </div>
          )}
          {!content &&
            hasData &&
            dataEntries.map(([k, v]) => (
              <div key={k}>
                <span style={{ color: 'var(--color-text-4)' }}>{k}:</span> {String(v)}
              </div>
            ))}
        </div>
      )}
    </div>
  );
}
