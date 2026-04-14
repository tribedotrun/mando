import React, { useRef, useState } from 'react';
import { useParams, useSearch } from '@tanstack/react-router';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { useQuery } from '@tanstack/react-query';
import { Copy, Check, Terminal as TerminalIcon } from 'lucide-react';
import { fetchTranscript } from '#renderer/api-sessions';
import { TranscriptViewer, formatCallerLabel } from '#renderer/domains/sessions';
import { useWorkbenchList } from '#renderer/hooks/queries';
import { copyToClipboard } from '#renderer/utils';
import { queryKeys } from '#renderer/queryKeys';
import { Button } from '#renderer/components/ui/button';
import { ScrollArea } from '#renderer/components/ui/scroll-area';
import { Skeleton } from '#renderer/components/ui/skeleton';
import { ErrorBoundary } from '#renderer/global/components/ErrorBoundary';
import { router } from '#renderer/app/router';

const COPY_FEEDBACK_MS = 1200;

export function TranscriptPage(): React.ReactElement {
  const { sessionId } = useParams({ strict: false }) as { sessionId: string };
  const search = useSearch({ from: '/_app/sessions/$sessionId' });

  const { data, isLoading, error } = useQuery({
    queryKey: queryKeys.sessions.transcript(sessionId),
    queryFn: () => fetchTranscript(sessionId),
    enabled: !!sessionId,
  });

  const [copyState, setCopyState] = useState<'idle' | 'copied'>('idle');
  const timerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useMountEffect(() => () => clearTimeout(timerRef.current));

  const resumeCmd = search.cwd
    ? `cd "${search.cwd}" && claude -r ${sessionId}`
    : `claude -r ${sessionId}`;

  const handleCopy = () => {
    void copyToClipboard(resumeCmd).then((ok) => {
      if (ok) {
        setCopyState('copied');
        clearTimeout(timerRef.current);
        timerRef.current = setTimeout(() => setCopyState('idle'), COPY_FEEDBACK_MS);
      }
    });
  };

  const { data: workbenches = [] } = useWorkbenchList();
  const workbench = search.cwd ? workbenches.find((w) => w.worktree === search.cwd) : null;

  const handleResumeInTerminal = () => {
    if (workbench) {
      void router.navigate({
        to: '/wb/$workbenchId',
        params: { workbenchId: String(workbench.id) },
        search: { tab: 'terminal', resume: sessionId },
      });
    }
  };

  const callerLabel = search.caller ? formatCallerLabel(search.caller) : 'session';
  const title = callerLabel.charAt(0).toUpperCase() + callerLabel.slice(1);

  return (
    <div className="absolute inset-0 flex flex-col overflow-hidden bg-background">
      {/* Header */}
      <div className="flex items-center gap-3 px-8 py-4">
        <div className="min-w-0 flex-1">
          <div className="text-subheading text-foreground">{title}</div>
          {search.taskTitle && (
            <div className="mt-0.5 text-caption text-muted-foreground">{search.taskTitle}</div>
          )}
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={handleCopy} className="gap-1.5">
            {copyState === 'copied' ? <Check size={13} /> : <Copy size={13} />}
            <span className="font-mono text-[11px]">-r</span>
          </Button>
          {workbench && (
            <Button
              variant="outline"
              size="sm"
              onClick={handleResumeInTerminal}
              className="gap-1.5"
            >
              <TerminalIcon size={13} />
              Resume in terminal
            </Button>
          )}
        </div>
      </div>

      {/* Transcript */}
      <ScrollArea className="min-h-0 flex-1 px-8 pb-6">
        <ErrorBoundary fallbackLabel="Transcript">
          {isLoading ? (
            <div className="space-y-3 py-4">
              <Skeleton className="h-5 w-48" />
              <Skeleton className="h-4 w-full" />
              <Skeleton className="h-4 w-3/4" />
              <Skeleton className="h-20 w-full" />
            </div>
          ) : error ? (
            <div
              className="rounded-md px-3 py-2 text-body"
              style={{
                background: 'color-mix(in srgb, var(--destructive) 10%, transparent)',
                color: 'var(--destructive)',
              }}
            >
              Failed to load transcript
            </div>
          ) : data?.markdown ? (
            <TranscriptViewer markdown={data.markdown} />
          ) : (
            <div className="py-8 text-center text-body text-muted-foreground">
              No transcript available
            </div>
          )}
        </ErrorBoundary>
      </ScrollArea>
    </div>
  );
}
