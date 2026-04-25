import { useEffect, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import type { ZodType } from 'zod';
import { openSseRoute } from '#renderer/global/providers/http';
import { eventSchemas } from '#shared/daemon-contract/schemas';
import { useTranscriptEventsCache } from '#renderer/domains/sessions/repo/queries';
import { parseSseMessage } from '#result';
import type {
  TranscriptEvent,
  TranscriptEventEnvelope,
  TranscriptEventsResponse,
} from '#renderer/global/types';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import log from '#renderer/global/service/logger';

export interface TranscriptEventsFeed {
  data: TranscriptEventsResponse | undefined;
  isLoading: boolean;
  error: Error | null;
}

/**
 * SSE is the sole data source for transcript events — there is no
 * parallel REST fetch. The server's SSE stream opens with a full
 * `snapshot` envelope (same shape the /events route would return),
 * then streams live `event` frames, then closes with
 * `connection_closed`. Using SSE as the single source eliminates the
 * snapshot-vs-snapshot race where a late REST response would overwrite
 * SSE-patched cache data and drop events written between the two reads.
 *
 * Cache reads go through the repo-tier `useTranscriptEventsCache` hook
 * (a disabled `useQuery` keyed on `queryKeys.sessions.events(sid)`);
 * writes happen here via `setQueryData` inside the SSE `onmessage`
 * handler. Loading/error state is local so the caller can render
 * skeletons until the `snapshot` envelope lands.
 */
export function useTranscriptEventsStream(sessionId: string | null): TranscriptEventsFeed {
  const queryClient = useQueryClient();
  const [isLoading, setIsLoading] = useState<boolean>(sessionId !== null);
  const [error, setError] = useState<Error | null>(null);
  const cached = useTranscriptEventsCache(sessionId);

  useEffect(() => {
    if (!sessionId) {
      setIsLoading(false);
      setError(null);
      return undefined;
    }
    setIsLoading(true);
    setError(null);

    const schema = (eventSchemas as Record<string, ZodType<unknown> | undefined>)
      .getSessionsByIdEventsStream;
    const cacheKey = queryKeys.sessions.events(sessionId);
    let source: EventSource;
    try {
      source = openSseRoute('getSessionsByIdEventsStream', { params: { id: sessionId } });
    } catch (err) {
      log.error('[transcript-sse] failed to open stream', err);
      setError(err instanceof Error ? err : new Error(String(err)));
      setIsLoading(false);
      return undefined;
    }

    source.onmessage = (message) => {
      const parsed = parseSseMessage(message.data as unknown, schema);
      if (parsed.failure) {
        log.warn('[transcript-sse] parse failure', parsed.failure);
        return;
      }
      const envelope = parsed.data as TranscriptEventEnvelope;
      // The server closes the stream once the session is no longer
      // running; without this the browser EventSource would auto-reconnect
      // (~3s default) forever, re-issuing the full snapshot each cycle.
      if (envelope.event === 'connection_closed') {
        source.close();
      }
      if (envelope.event === 'snapshot' || envelope.event === 'snapshot_complete') {
        setIsLoading(false);
      }
      if (envelope.event === 'error') {
        setError(new Error(envelope.data.message));
      }
      queryClient.setQueryData<TranscriptEventsResponse>(cacheKey, (prev) =>
        applyEnvelope(prev, envelope, sessionId),
      );
    };

    source.onerror = () => {
      log.warn('[transcript-sse] connection error — EventSource will retry');
    };

    return () => source.close();
  }, [sessionId, queryClient]);

  return { data: cached.data, isLoading, error };
}

function applyEnvelope(
  prev: TranscriptEventsResponse | undefined,
  envelope: TranscriptEventEnvelope,
  sessionId: string,
): TranscriptEventsResponse {
  const base: TranscriptEventsResponse = prev ?? {
    sessionId,
    events: [],
    isRunning: false,
  };
  switch (envelope.event) {
    case 'snapshot': {
      const next: TranscriptEvent[] = envelope.data.events;
      return { ...base, events: next };
    }
    case 'snapshot_complete':
      return { ...base, isRunning: envelope.data.isRunning };
    case 'event':
      return { ...base, events: [...base.events, envelope.data] };
    case 'connection_closed':
      return { ...base, isRunning: false };
    case 'error':
      log.warn('[transcript-sse] stream error', envelope.data);
      return base;
  }
}
