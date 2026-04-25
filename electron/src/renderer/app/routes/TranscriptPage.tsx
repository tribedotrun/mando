import React from 'react';
import { useParams, useSearch } from '@tanstack/react-router';
import { Copy, Check, FileText, Terminal as TerminalIcon } from 'lucide-react';
import {
  formatCallerLabel,
  useSessionJsonlPath,
  buildResumeCmd,
  isTranscriptUnavailable,
  useTranscriptEventsStream,
} from '#renderer/domains/sessions';
import { TranscriptMessageList } from '#renderer/domains/sessions/ui/transcriptEvents/TranscriptMessageList';
import { useWorkbenchList } from '#renderer/domains/captain';
import { useNativeActions } from '#renderer/global/runtime/useFeedbackNativeActions';
import { copyToClipboard } from '#renderer/global/runtime/useFeedback';
import { useCopyFeedback } from '#renderer/global/runtime/useCopyFeedback';
import { Button } from '#renderer/global/ui/primitives/button';
import { Skeleton } from '#renderer/global/ui/primitives/skeleton';
import { ErrorBoundary } from '#renderer/global/ui/ErrorBoundary';
import { router } from '#renderer/app/router';

export function TranscriptPage(): React.ReactElement {
  const { sessionId } = useParams({ strict: false }) as { sessionId: string };
  const search = useSearch({ from: '/_app/sessions/$sessionId' });

  const { data, isLoading, error } = useTranscriptEventsStream(sessionId);

  const { copied, markCopied } = useCopyFeedback();

  const resumeCmd = buildResumeCmd(sessionId, search.cwd);

  const handleCopy = () => {
    void (async () => {
      const ok = await copyToClipboard(resumeCmd);
      if (ok) markCopied();
    })();
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

  const { data: jsonl } = useSessionJsonlPath(sessionId);
  const jsonlPath = jsonl?.path ?? null;
  const { files } = useNativeActions();

  const handleOpenJsonl = () => {
    if (!jsonlPath) return;
    files.openLocalPath(jsonlPath);
  };

  const title = search.caller ? formatCallerLabel(search.caller) : 'Session';

  return (
    <div className="absolute inset-0 flex flex-col overflow-hidden bg-background">
      {/* Header */}
      <div className="flex items-center gap-3 px-8 pt-2 pb-4">
        <div className="min-w-0 flex-1">
          <div className="text-subheading text-foreground">{title}</div>
          {search.taskTitle && (
            <div className="mt-0.5 text-caption text-muted-foreground">{search.taskTitle}</div>
          )}
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={handleCopy} className="gap-1.5">
            {copied ? <Check size={13} /> : <Copy size={13} />}
            <span className="font-mono text-[11px]">-r</span>
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={handleOpenJsonl}
            disabled={!jsonlPath}
            title={jsonlPath ? jsonlPath : 'JSONL file not available for this session'}
            className="gap-1.5"
          >
            <FileText size={13} />
            Open JSONL
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
      <ErrorBoundary fallbackLabel="Transcript">
        {isLoading ? (
          <div className="space-y-3 px-8 py-4">
            <Skeleton className="h-5 w-48" />
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-3/4" />
            <Skeleton className="h-20 w-full" />
          </div>
        ) : error ? (
          isTranscriptUnavailable(error) ? (
            <div className="mx-8 rounded-md border border-dashed px-3 py-3 text-body text-muted-foreground">
              No transcript was recorded for this session. This usually means the session failed or
              was killed before emitting any output.
            </div>
          ) : (
            <div
              className="mx-8 rounded-md px-3 py-2 text-body"
              style={{
                background: 'color-mix(in srgb, var(--destructive) 10%, transparent)',
                color: 'var(--destructive)',
              }}
            >
              Failed to load transcript
            </div>
          )
        ) : data?.events && data.events.length > 0 ? (
          <TranscriptMessageList events={data.events} isRunning={data.isRunning} />
        ) : (
          <div className="py-8 text-center text-body text-muted-foreground">
            No transcript available
          </div>
        )}
      </ErrorBoundary>
    </div>
  );
}
