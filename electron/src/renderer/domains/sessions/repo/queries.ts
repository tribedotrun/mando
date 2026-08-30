import { keepPreviousData, skipToken, useQuery } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { daemonSyncMeta } from '#renderer/global/repo/syncPolicy';
import {
  fetchSession,
  fetchSessionJsonlPath,
  fetchSessions,
} from '#renderer/domains/sessions/repo/api';
import type {
  SessionCategory,
  SessionEntry,
  SessionsListResponse,
  SessionStatus,
  TranscriptEventsResponse,
} from '#renderer/global/types';
import { toReactQuery } from '#result';

export function useSessionsList(
  page: number,
  category?: SessionCategory,
  status?: SessionStatus,
  perPage = 50,
) {
  return useQuery<SessionsListResponse>({
    queryKey: queryKeys.sessions.list(page, category, status, perPage),
    meta: daemonSyncMeta('sse-invalidated', 'session events invalidate session lists'),
    queryFn: () => toReactQuery(fetchSessions(page, perPage, category, status)),
    // Only retain stale data while paginating (page > 1); clear immediately on
    // filter changes (page resets to 1) so stale mismatched rows are not shown.
    placeholderData: page > 1 ? keepPreviousData : undefined,
  });
}

export function useSession(sessionId: string | null) {
  const keySessionId = sessionId ?? '__disabled__';
  return useQuery<SessionEntry>({
    queryKey: queryKeys.sessions.detail(keySessionId),
    meta: daemonSyncMeta('manual', 'single session metadata for transcript routes'),
    queryFn: () => toReactQuery(fetchSession(sessionId!)),
    enabled: !!sessionId,
  });
}

/**
 * Cache-only reader for typed transcript events. The cache is written
 * exclusively by `useTranscriptEventsStream` via `setQueryData` — this
 * hook never fires a REST fetch (enforced by `skipToken`). Keeping the
 * entry under `queryKeys.sessions.events(sid)` gives devtools + the
 * daemon-sync registry a single place to observe the live-tail state.
 */
export function useTranscriptEventsCache(sessionId: string | null) {
  const keySessionId = sessionId ?? '__disabled__';
  return useQuery<TranscriptEventsResponse>({
    queryKey: queryKeys.sessions.events(keySessionId),
    queryFn: skipToken,
    enabled: false,
    meta: daemonSyncMeta('sse-patched', 'transcript events hydrated by SSE stream'),
  });
}

export function useSessionJsonlPath(sessionId: string | null) {
  const keySessionId = sessionId ?? '__disabled__';
  return useQuery({
    queryKey: queryKeys.sessions.jsonlPath(keySessionId),
    meta: daemonSyncMeta('manual', 'jsonl path resolved on transcript selection'),
    queryFn: () => toReactQuery(fetchSessionJsonlPath(sessionId!)),
    enabled: !!sessionId,
  });
}
