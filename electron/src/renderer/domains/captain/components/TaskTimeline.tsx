import React, { useState } from 'react';
import type { TimelineEvent } from '#renderer/types';
import { relativeTime, localizeMeta } from '#renderer/utils';
import { StatusIcon } from '#renderer/global/components/StatusIndicator';
import { PrIcon } from '#renderer/global/components/icons';
import { Button } from '#renderer/components/ui/button';
import { Commit } from '#renderer/components/ui/commit';

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
  auto_merge_triage: 'captain-reviewing',
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
  worker_reopened: 'captain-reviewing',
};

/** Human-readable label for auto-reopens based on source. */
function reopenLabel(data: Record<string, unknown>): string {
  const source = data.source as string | undefined;
  if (source === 'review') return 'review reopened';
  if (source === 'ci') return 'CI reopened';
  if (source === 'evidence') return 'evidence reopened';
  return 'auto reopened';
}

/** Keys to always exclude from expanded detail view. */
const HIDDEN_KEYS = new Set(['session_id', 'source', 'item_id', 'task_id']);

/** Event types whose summary already contains the worker name, making the worker detail redundant. */
const WORKER_IN_SUMMARY = new Set(['worker_spawned', 'session_resumed']);

/** Values that add no information -- skip them in detail view. */
function isUselessValue(value: unknown): boolean {
  if (value === null || value === undefined || value === '') return true;
  const s = String(value).trim();
  if (s.length === 0) return true;
  // Single-word uppercase status that duplicates the event type
  if (/^[A-Z_]+$/.test(s) && s.length < 40) return true;
  return false;
}

/** Detect a hex SHA (7-40 chars). */
const SHA_RE = /^[0-9a-f]{7,40}$/i;

/** Check if a key name suggests a commit SHA. */
function isShaField(key: string, value: string): boolean {
  const k = key.toLowerCase();
  return (
    (k.includes('sha') || k.includes('commit') || k.includes('head')) && SHA_RE.test(value.trim())
  );
}

/** Get useful detail entries from event data. */
function getUsefulDetails(data: Record<string, unknown>, eventType: string): [string, string][] {
  return Object.entries(data)
    .filter(
      ([k, v]) =>
        !HIDDEN_KEYS.has(k) &&
        !isUselessValue(v) &&
        !(k === 'worker' && WORKER_IN_SUMMARY.has(eventType)),
    )
    .map(([k, v]) => {
      let display = String(v);
      // Shorten GitHub URLs to repo#number format
      const prMatch = display.match(/github\.com\/([^/]+\/[^/]+)\/pull\/(\d+)/);
      if (prMatch) display = `${prMatch[1]}#${prMatch[2]}`;
      return [k, localizeMeta(display)] as [string, string];
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
    return <div className="text-caption text-text-3">No timeline events</div>;
  }

  return (
    <div>
      {events.map((event, i) => {
        const sessionId = event.data.session_id as string | undefined;
        const showTranscript = sessionId && !shownSessionIds.has(sessionId);
        if (sessionId) shownSessionIds.add(sessionId);
        const isExpanded = expandedIdx === i;
        const details = getUsefulDetails(event.data, event.event_type);
        const hasDetails = details.length > 0;
        const isMerged = event.event_type === 'merged';

        return (
          <div key={`${event.timestamp}-${i}`} className="mb-0.5">
            <div
              role={hasDetails ? 'button' : undefined}
              tabIndex={hasDetails ? 0 : undefined}
              className={`flex items-center gap-2 rounded-md px-2 py-1 transition-colors hover:bg-muted${hasDetails ? ' cursor-pointer active:bg-muted/80' : ''}`}
              onClick={() => hasDetails && setExpandedIdx(isExpanded ? null : i)}
              onKeyDown={(e) => {
                if (hasDetails && (e.key === 'Enter' || e.key === ' ')) {
                  e.preventDefault();
                  setExpandedIdx(isExpanded ? null : i);
                }
              }}
            >
              {/* Icon */}
              <span className="flex w-4 shrink-0 items-center justify-center">
                {isMerged ? (
                  <PrIcon state="merged" />
                ) : (
                  <StatusIcon status={EVENT_ICON_MAP[event.event_type] ?? 'queued'} />
                )}
              </span>
              <span className="text-caption font-medium text-foreground">
                {event.event_type === 'worker_reopened'
                  ? reopenLabel(event.data)
                  : event.event_type.replace(/_/g, ' ')}
              </span>
              <span className="min-w-0 flex-1 truncate text-caption text-text-3">
                {localizeMeta(event.summary)}
              </span>
              {showTranscript && (
                <Button
                  variant="ghost"
                  size="xs"
                  onClick={(e) => {
                    e.stopPropagation();
                    onTranscriptClick(sessionId, event);
                  }}
                  className="shrink-0 text-muted-foreground"
                >
                  transcript
                </Button>
              )}
              <span className="shrink-0 text-caption text-text-4">
                {relativeTime(event.timestamp)}
              </span>
            </div>
            {isExpanded && hasDetails && (
              <div className="ml-6 mt-1 break-words rounded-md bg-muted px-3 py-2 text-caption text-muted-foreground">
                {details.map(([k, v]) => (
                  <div key={k} className="mb-0.5">
                    <span className="text-text-4">{k}:</span>{' '}
                    {isShaField(k, v) ? <Commit hash={v} message="" /> : v}
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
