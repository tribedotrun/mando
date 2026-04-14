import React, { useState } from 'react';
import { Check, Copy, X } from 'lucide-react';
import type { TaskItem, SessionSummary, TimelineEvent } from '#renderer/types';
import { copyToClipboard, fmtDuration, relativeTime, shortenPath } from '#renderer/utils';
import { PrSections } from '#renderer/domains/captain/components/PrSections';
import { PrMarkdown } from '#renderer/domains/captain/components/PrMarkdown';
import { TaskTimeline } from '#renderer/domains/captain/components/TaskTimeline';
import { formatCallerLabel, buildSessionSequence, SessionDot } from '#renderer/domains/sessions';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogClose,
} from '#renderer/components/ui/dialog';
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
  TooltipProvider,
} from '#renderer/components/ui/tooltip';
import { Button } from '#renderer/components/ui/button';
import { Skeleton } from '#renderer/components/ui/skeleton';

const COPY_FEEDBACK_MS = 1200;

/* -- Timeline tab -- */

export function TimelineTab({
  events,
  onTranscriptClick,
}: {
  events: TimelineEvent[];
  onTranscriptClick: (sessionId: string, event: TimelineEvent) => void;
}): React.ReactElement {
  const reversed = [...events].reverse();
  return <TaskTimeline events={reversed} onTranscriptClick={onTranscriptClick} />;
}

/* -- PR tab -- */

export function PrTab({
  item,
  prBody,
  prPending,
}: {
  item: TaskItem;
  prBody: { summary: string | null } | undefined;
  prPending: boolean;
}): React.ReactElement {
  if (!item.pr_number) {
    return <div className="text-caption text-text-3">No PR associated with this task</div>;
  }
  if (prPending && !prBody) {
    return (
      <div className="min-h-[120px] space-y-3">
        <Skeleton className="h-4 w-3/4" />
        <Skeleton className="h-4 w-1/2" />
        <Skeleton className="h-4 w-2/3" />
      </div>
    );
  }
  if (!prBody?.summary) {
    return <div className="text-caption italic text-text-3">No PR description available</div>;
  }
  return <PrSections text={prBody.summary} />;
}

/* -- Sessions tab -- */

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

  // buildSessionSequence expects newest-first input (it reverses internally).
  const seqMap = buildSessionSequence(
    reversed.map((s) => ({
      session_id: s.session_id,
      created_at: s.started_at || '',
      cwd: s.cwd || '',
      model: s.model || '',
      caller: s.caller,
      resumed: s.resumed ? 1 : 0,
      task_id: String(taskId),
      worker_name: s.worker_name || '',
      status: s.status,
    })),
  );

  return (
    <div>
      <div className="mb-2 text-caption text-text-4">Sessions</div>
      <div className="space-y-0.5">
        {reversed.map((s) => {
          const label =
            formatCallerLabel(s.caller).charAt(0).toUpperCase() +
            formatCallerLabel(s.caller).slice(1);
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
                <div className="text-body-sm font-medium text-text-1">
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

/* -- Info tab -- */

export function InfoTab({ item }: { item: TaskItem }): React.ReactElement {
  return (
    <div className="space-y-5">
      <div className="grid grid-cols-[auto_1fr] items-baseline gap-x-6 gap-y-2.5">
        <span className="text-caption text-text-4">ID</span>
        <span className="font-mono text-caption text-text-2">#{item.id}</span>

        {item.worktree && (
          <>
            <span className="text-caption text-text-4">Worktree</span>
            <CopyValue value={item.worktree} display={shortenPath(item.worktree)} />
          </>
        )}

        {item.branch && (
          <>
            <span className="text-caption text-text-4">Branch</span>
            <CopyValue value={item.branch} />
          </>
        )}

        {item.plan && (
          <>
            <span className="text-caption text-text-4">Plan</span>
            <CopyValue value={item.plan} display={shortenPath(item.plan)} />
          </>
        )}
      </div>

      {item.original_prompt && (
        <div>
          <div className="mb-1.5 text-caption text-text-4">Original Request</div>
          <p className="text-body-sm leading-relaxed text-text-2">{item.original_prompt}</p>
        </div>
      )}
    </div>
  );
}

function CopyValue({ value, display }: { value: string; display?: string }): React.ReactElement {
  const [copied, setCopied] = useState(false);
  const [copying, setCopying] = useState(false);
  return (
    <span className="inline-flex items-center gap-2 text-code text-muted-foreground">
      <span className="min-w-0 break-all">{display ?? value}</span>
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon-xs"
              disabled={copying}
              onClick={() => {
                setCopying(true);
                void copyToClipboard(value)
                  .then((ok) => {
                    if (ok) {
                      setCopied(true);
                      setTimeout(() => setCopied(false), COPY_FEEDBACK_MS);
                    }
                  })
                  .finally(() => setCopying(false));
              }}
              className="h-5 w-5"
            >
              {copied ? (
                <Check size={12} color="var(--success)" />
              ) : (
                <Copy size={12} color="var(--text-4)" />
              )}
            </Button>
          </TooltipTrigger>
          <TooltipContent>{copied ? 'Copied!' : 'Copy'}</TooltipContent>
        </Tooltip>
      </TooltipProvider>
    </span>
  );
}

/* -- Context modal -- */

export function ContextModal({
  context,
  onClose,
}: {
  context: string;
  onClose: () => void;
}): React.ReactElement {
  return (
    <Dialog
      open={true}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent
        data-testid="context-modal"
        className="flex max-h-[70vh] w-[560px] max-w-[90vw] flex-col p-0"
        showCloseButton={false}
      >
        <div className="flex shrink-0 items-center justify-between px-5 pt-4 pb-3">
          <DialogHeader className="flex-1">
            <DialogTitle className="mb-0">Context</DialogTitle>
          </DialogHeader>
          <DialogClose asChild>
            <Button variant="ghost" size="icon-xs">
              <X size={14} />
            </Button>
          </DialogClose>
        </div>
        <div className="min-w-0 overflow-y-auto px-5 pb-5 [overflow-wrap:anywhere]">
          <PrMarkdown text={context} />
        </div>
      </DialogContent>
    </Dialog>
  );
}
