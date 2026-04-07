import React, { useRef } from 'react';
import { ChevronLeft } from 'lucide-react';
import type { SessionEntry } from '#renderer/types';
import { TranscriptViewer } from '#renderer/domains/sessions/components/TranscriptViewer';
import { fmtDuration, copyToClipboard } from '#renderer/utils';
import { sessionTitle } from '#renderer/domains/sessions/components/SessionsHelpers';
import { Button } from '#renderer/components/ui/button';

import { ScrollArea } from '#renderer/components/ui/scroll-area';
import { Skeleton } from '#renderer/components/ui/skeleton';

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

  const copyResume = async () => {
    const ok = await copyToClipboard(resumeCmd);
    if (ok && copyRef.current) {
      copyRef.current.textContent = 'copied!';
      setTimeout(() => {
        if (copyRef.current) copyRef.current.textContent = 'resume';
      }, 1200);
    }
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
    <div data-testid="session-detail" className="flex min-h-0 h-full flex-col">
      {/* Header bar */}
      <div className="flex items-center gap-2 pb-4">
        <Button variant="ghost" size="icon-xs" onClick={onClose}>
          <ChevronLeft size={16} />
        </Button>
        <div className="min-w-0 flex-1">
          <div className="truncate text-subheading text-foreground">{title}</div>
          <div className="mt-0.5 flex min-w-0 items-center gap-2 text-caption text-muted-foreground">
            {subtitleParts}
          </div>
        </div>
        <Button ref={copyRef} variant="outline" size="sm" onClick={copyResume}>
          resume
        </Button>
      </div>

      {/* Transcript -- scrollable area */}
      <ScrollArea className="flex-1 rounded-lg bg-card px-5 py-4">
        {loading ? (
          <div className="space-y-3">
            <Skeleton className="h-5 w-48" />
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-3/4" />
            <Skeleton className="h-4 w-5/6" />
            <Skeleton className="h-20 w-full" />
            <Skeleton className="h-4 w-2/3" />
          </div>
        ) : error ? (
          <div
            className="rounded-md px-3 py-2 text-body"
            style={{
              background: 'color-mix(in srgb, var(--destructive) 10%, transparent)',
              color: 'var(--destructive)',
            }}
          >
            {error}
          </div>
        ) : markdown ? (
          <TranscriptViewer markdown={markdown} />
        ) : (
          <div className="text-body text-muted-foreground">No transcript available</div>
        )}
      </ScrollArea>
    </div>
  );
}
