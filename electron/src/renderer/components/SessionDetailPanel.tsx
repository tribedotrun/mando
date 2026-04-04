import React, { useRef } from 'react';
import type { SessionEntry } from '#renderer/types';
import { TranscriptViewer } from '#renderer/components/TranscriptViewer';
import { fmtDuration } from '#renderer/utils';
import { sessionTitle } from '#renderer/components/SessionsHelpers';

interface Props {
  session: SessionEntry;
  markdown: string | null;
  loading: boolean;
  error: string | null;
  onClose: () => void;
  resumeCmd: string;
  sequenceNum?: number;
}

function formatTimestamp(ts: string): string {
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return ts;
  const now = new Date();
  const isToday = d.toDateString() === now.toDateString();
  const time = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  if (isToday) return `Today ${time}`;
  const yesterday = new Date(now);
  yesterday.setDate(yesterday.getDate() - 1);
  if (d.toDateString() === yesterday.toDateString()) return `Yesterday ${time}`;
  return `${d.toLocaleDateString([], { month: 'short', day: 'numeric' })} ${time}`;
}

export function SessionDetailPanel({
  session,
  markdown,
  loading,
  error,
  onClose,
  resumeCmd,
  sequenceNum,
}: Props): React.ReactElement {
  const copyRef = useRef<HTMLButtonElement>(null);

  const durationSec = session.duration_ms != null ? session.duration_ms / 1000 : undefined;
  const baseTitle = sessionTitle(session);
  const title = sequenceNum ? `${baseTitle} #${sequenceNum}` : baseTitle;

  const copyResume = () => {
    navigator.clipboard.writeText(resumeCmd).then(
      () => {
        if (copyRef.current) {
          copyRef.current.textContent = 'copied!';
          setTimeout(() => {
            if (copyRef.current) copyRef.current.textContent = 'resume';
          }, 1200);
        }
      },
      () => {
        // Clipboard access denied
      },
    );
  };

  const subtitleParts: React.ReactNode[] = [];
  const contextLabel = session.task_title || session.scout_item_title;
  if (contextLabel) {
    subtitleParts.push(
      <span key="context" className="truncate">
        {contextLabel}
      </span>,
    );
  }
  if (session.task_id) {
    subtitleParts.push(<span key="task">{session.task_id}</span>);
  }
  if (session.created_at) {
    subtitleParts.push(<span key="time">{formatTimestamp(session.created_at)}</span>);
  }
  if (durationSec != null) {
    subtitleParts.push(<span key="dur">{fmtDuration(durationSec)}</span>);
  }

  return (
    <div data-testid="session-detail" className="flex h-full flex-col" style={{ minHeight: 0 }}>
      {/* Header bar */}
      <div className="flex items-center gap-2 pb-4">
        <button
          onClick={onClose}
          className="shrink-0 rounded p-1"
          style={{
            color: 'var(--color-text-3)',
            background: 'none',
            border: 'none',
            cursor: 'pointer',
          }}
          title="Back to sessions"
        >
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
            <path
              d="M10 3L5 8l5 5"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        </button>
        <div className="min-w-0 flex-1">
          <div className="truncate text-subheading" style={{ color: 'var(--color-text-1)' }}>
            {title}
          </div>
          <div
            className="mt-0.5 flex min-w-0 items-center gap-2 text-caption"
            style={{ color: 'var(--color-text-3)' }}
          >
            {subtitleParts}
          </div>
        </div>
        <button
          ref={copyRef}
          onClick={copyResume}
          className="shrink-0 whitespace-nowrap rounded border px-3 py-1.5 text-xs"
          style={{ borderColor: 'var(--color-border)', color: 'var(--color-text-2)' }}
        >
          resume
        </button>
      </div>

      {/* Transcript — scrollable area */}
      <div
        className="flex-1 overflow-auto rounded-lg px-5 py-4"
        style={{ background: 'var(--color-surface-1)' }}
      >
        {loading ? (
          <div className="text-body" style={{ color: 'var(--color-text-3)' }}>
            Loading transcript...
          </div>
        ) : error ? (
          <div
            className="rounded border px-3 py-2 text-body"
            style={{
              borderColor: 'color-mix(in srgb, var(--color-error) 30%, transparent)',
              background: 'color-mix(in srgb, var(--color-error) 10%, transparent)',
              color: 'var(--color-error)',
            }}
          >
            {error}
          </div>
        ) : markdown ? (
          <TranscriptViewer markdown={markdown} />
        ) : (
          <div className="text-body" style={{ color: 'var(--color-text-3)' }}>
            No transcript available
          </div>
        )}
      </div>
    </div>
  );
}
