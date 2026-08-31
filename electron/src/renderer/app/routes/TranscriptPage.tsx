import React from 'react';
import { useParams, useSearch } from '@tanstack/react-router';
import { Copy, Check, FileText } from 'lucide-react';
import {
  formatCallerLabel,
  useSessionJsonlPath,
  buildResumeCmd,
  useTranscriptEventsStream,
  useTranscriptRouteContext,
} from '#renderer/domains/sessions';
import { TranscriptContent } from '#renderer/domains/sessions/ui/transcriptEvents/TranscriptContent';
import { TranscriptTokenUsage } from '#renderer/domains/sessions/ui/transcriptEvents/TranscriptTokenUsage';
import { useNativeActions } from '#renderer/global/runtime/useNativeActions';
import { copyToClipboard } from '#renderer/global/runtime/useFeedback';
import { useCopyFeedback } from '#renderer/global/runtime/useCopyFeedback';
import { Button } from '#renderer/global/ui/primitives/button';

export function TranscriptPage(): React.ReactElement {
  const { sessionId } = useParams({ strict: false }) as { sessionId: string };
  const search = useSearch({ from: '/_app/sessions/$sessionId' });

  const { data, isLoading, error } = useTranscriptEventsStream(sessionId);
  const {
    provider,
    cwd: transcriptCwd,
    isLoading: isSessionContextLoading,
  } = useTranscriptRouteContext(sessionId, search.provider, search.cwd);

  const { copied, markCopied } = useCopyFeedback();
  const resumeCmd = buildResumeCmd(sessionId, provider, transcriptCwd);

  const handleCopy = () => {
    if (!resumeCmd) return;
    void (async () => {
      const ok = await copyToClipboard(resumeCmd);
      if (ok) markCopied();
    })();
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
      <div className="flex items-center gap-3 px-8 pt-2 pb-4">
        <div className="min-w-0 flex-1">
          <div className="text-subheading text-foreground">{title}</div>
          {search.taskTitle && (
            <div className="mt-0.5 text-caption text-muted-foreground">{search.taskTitle}</div>
          )}
          <TranscriptTokenUsage events={data?.events} isLoading={isLoading} />
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={handleCopy}
            disabled={!resumeCmd}
            title={
              isSessionContextLoading
                ? 'Loading session metadata'
                : resumeCmd
                  ? `Copy ${
                      provider === 'codex'
                        ? 'Codex'
                        : provider === 'opencode'
                          ? 'OpenCode'
                          : 'Claude Code'
                    } resume command`
                  : 'Session metadata is unavailable'
            }
            className="gap-1.5"
          >
            {copied ? <Check size={13} /> : <Copy size={13} />}
            <span className="font-mono text-[11px]">
              {provider === 'codex' ? 'Codex' : provider === 'opencode' ? 'OC' : '-r'}
            </span>
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
        </div>
      </div>

      <TranscriptContent data={data} isLoading={isLoading} error={error} />
    </div>
  );
}
