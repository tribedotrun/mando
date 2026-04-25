import React from 'react';
import type { SessionSummary } from '#renderer/global/types';
import { fmtDuration, relativeTime } from '#renderer/global/service/utils';
import { formatCallerLabel, buildSequenceFromSummaries } from '#renderer/domains/sessions';
import { SessionDot } from '#renderer/global/ui/SessionDot';
import { Button } from '#renderer/global/ui/primitives/button';

export function SessionsTab({
  sessions,
  onSessionClick,
  onResumeSession,
  taskId,
}: {
  sessions: SessionSummary[];
  onSessionClick: (s: SessionSummary) => void;
  onResumeSession?: (sessionId: string, name?: string) => void;
  taskId: number;
}): React.ReactElement {
  if (sessions.length === 0) {
    return <div className="text-caption text-text-3">No sessions yet</div>;
  }

  const totalDuration = sessions.reduce((s, x) => s + (x.duration_ms ?? 0), 0);
  const reversed = [...sessions].reverse();

  const seqMap = buildSequenceFromSummaries(reversed, taskId);

  return (
    <div>
      <div className="mb-2 text-caption text-text-4">Sessions</div>
      <div className="space-y-0.5">
        {reversed.map((s) => {
          const label = formatCallerLabel(s.caller);
          const seq = seqMap.get(s.session_id);
          const title = seq ? `${label} #${seq}` : label;

          return (
            <div
              key={s.session_id}
              role="button"
              tabIndex={0}
              onClick={() => onSessionClick(s)}
              className="group flex w-full cursor-pointer items-center gap-2.5 rounded-md px-2 py-2 text-left transition-colors hover:bg-muted"
            >
              <SessionDot status={s.status} />
              <div className="min-w-0 flex-1">
                <div
                  className="truncate text-body font-medium text-text-1"
                  title={title + (s.worker_name ? ` (${s.worker_name})` : '')}
                >
                  {title}
                  {s.worker_name ? ` (${s.worker_name})` : ''}
                </div>
                <div className="text-caption text-text-3">
                  {s.started_at && <span>{relativeTime(s.started_at)}</span>}
                  {s.model && <span> &middot; {s.model}</span>}
                  {s.duration_ms != null && s.duration_ms > 0 && (
                    <span> &middot; {fmtDuration(s.duration_ms / 1000)}</span>
                  )}
                </div>
              </div>
              {onResumeSession && s.status !== 'running' ? (
                <Button
                  variant="outline"
                  size="xs"
                  onClick={(e) => {
                    e.stopPropagation();
                    const displayName = title + (s.worker_name ? ` (${s.worker_name})` : '');
                    onResumeSession(s.session_id, displayName);
                  }}
                  className="opacity-0 transition-opacity group-hover:opacity-100"
                  title="Resume this session in a terminal"
                >
                  Resume
                </Button>
              ) : (
                <span className="text-[11px] text-text-4">{s.status}</span>
              )}
            </div>
          );
        })}
      </div>

      <div className="px-2 pt-2 text-caption text-text-4">
        {sessions.length} sessions
        {totalDuration > 0 && <span> &middot; {fmtDuration(totalDuration / 1000)}</span>}
      </div>
    </div>
  );
}
