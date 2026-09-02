import React from 'react';
import {
  formatExactTokenCount,
  formatUsageBreakdown,
  summarizeTranscriptTokenUsage,
} from '#renderer/domains/sessions/service/transcriptTokenUsage';
import type { TaskProvider, TranscriptEvent } from '#renderer/global/types';

interface TranscriptTokenUsageProps {
  events: readonly TranscriptEvent[] | undefined;
  isLoading: boolean;
  provider?: TaskProvider;
}

export function TranscriptTokenUsage({
  events,
  isLoading,
  provider,
}: TranscriptTokenUsageProps): React.ReactElement {
  const usage = events ? summarizeTranscriptTokenUsage(events, provider) : null;
  const value = isLoading ? '…' : usage ? formatExactTokenCount(usage.totalTokens) : '—';
  const title = usage ? formatUsageBreakdown(usage) : 'Token usage unavailable for this session';

  return (
    <div
      className="mt-1 flex items-center gap-1.5 text-label uppercase tracking-wider text-text-3"
      title={title}
      aria-label={`Tokens used ${value}`}
    >
      <span>tokens used</span>
      <span className="font-mono text-muted-foreground">{value}</span>
    </div>
  );
}
