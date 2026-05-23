import React from 'react';
import { isTranscriptUnavailable } from '#renderer/domains/sessions/service/helpers';
import { TranscriptMessageList } from '#renderer/domains/sessions/ui/transcriptEvents/TranscriptMessageList';
import type { TranscriptEventsResponse } from '#renderer/global/types';
import { ErrorBoundary } from '#renderer/global/ui/ErrorBoundary';
import { Skeleton } from '#renderer/global/ui/primitives/skeleton';

interface TranscriptContentProps {
  data: TranscriptEventsResponse | undefined;
  isLoading: boolean;
  error: Error | null;
}

export function TranscriptContent({
  data,
  isLoading,
  error,
}: TranscriptContentProps): React.ReactElement {
  return (
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
  );
}
