import type {
  TaskListResponse,
  TaskItem,
  WorkersResponse,
  SessionsResponse,
  TranscriptResponse,
  TimelineResponse,
  TickResult,
  PrSummaryResponse,
  AskResponse,
  AskHistoryResponse,
  SSEConnectionStatus,
  ItemSessionsResponse,
} from '#renderer/types';
import log from '#renderer/logger';
import { getErrorMessage } from '#renderer/utils';

// Re-export submodules so consumers can keep importing from '#renderer/api'
export type { ScoutQueryParams } from '#renderer/api-scout';
export {
  fetchScoutItems,
  fetchScoutItem,
  fetchScoutArticle,
  addScoutUrl,
  updateScoutStatus,
  bulkUpdateScout,
  bulkDeleteScout,
  askScout,
  actOnScoutItem,
  researchScout,
  publishScoutTelegraph,
  fetchScoutItemSessions,
} from '#renderer/api-scout';

let BASE_URL = 'http://127.0.0.1:18893';
export async function initBaseUrl(): Promise<void> {
  if (window.mandoAPI) {
    const url = await window.mandoAPI.gatewayUrl();
    if (url) BASE_URL = url;
  }
}

export function buildUrl(path: string): string {
  return `${BASE_URL}${path}`;
}

// ---------------------------------------------------------------------------
// Error batching — queues errors and POSTs to /api/client-logs every 5s
// ---------------------------------------------------------------------------

/** Distinguishes HTTP errors (already logged) from network errors. */
class HttpError extends Error {
  constructor(
    message: string,
    public status: number,
  ) {
    super(message);
  }
}

interface ClientLogEntry {
  level: string;
  message: string;
  context?: unknown;
  timestamp: string;
}

let errorBatch: ClientLogEntry[] = [];
let batchTimer: ReturnType<typeof setTimeout> | null = null;
let flushFailures = 0;
let degradationReported = false;

const MAX_ERROR_BATCH = 200;
const MAX_FLUSH_RETRIES = 5;
const BASE_RETRY_MS = 5_000;
const MAX_RETRY_MS = 60_000;
const FLUSH_DELAY_MS = 5_000;

/** Fired once when the client-logs flush exceeds MAX_FLUSH_RETRIES consecutive failures. */
export const OBS_DEGRADED_EVENT = 'mando:obs-degraded';

function queueError(level: string, message: string, context?: unknown): void {
  if (errorBatch.length >= MAX_ERROR_BATCH) return;
  errorBatch.push({
    level,
    message,
    context,
    timestamp: new Date().toISOString(),
  });

  if (!batchTimer) {
    batchTimer = setTimeout(() => void flushErrors(), FLUSH_DELAY_MS);
  }
}

async function flushErrors(): Promise<void> {
  batchTimer = null;
  if (errorBatch.length === 0) return;

  const entries = [...errorBatch];
  errorBatch = [];

  try {
    await fetch(`${BASE_URL}/api/client-logs`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ entries }),
    });
    flushFailures = 0;
  } catch (err) {
    flushFailures++;
    const reason = getErrorMessage(err, 'unknown');
    if (flushFailures >= MAX_FLUSH_RETRIES) {
      log.error(
        `[obs] dropping ${entries.length} entries after ${MAX_FLUSH_RETRIES} flush failures (last error: ${reason})`,
      );
      if (!degradationReported && typeof window !== 'undefined') {
        degradationReported = true;
        window.dispatchEvent(new CustomEvent(OBS_DEGRADED_EVENT));
      }
      flushFailures = 0;
      return;
    }
    log.warn(
      `[obs] flush failed (attempt ${flushFailures}/${MAX_FLUSH_RETRIES}, ${reason}), will retry`,
    );
    errorBatch.push(...entries.slice(0, MAX_ERROR_BATCH - errorBatch.length));
    if (!batchTimer && errorBatch.length > 0) {
      const delay = Math.min(BASE_RETRY_MS * 2 ** flushFailures, MAX_RETRY_MS);
      batchTimer = setTimeout(() => void flushErrors(), delay);
    }
  }
}

// ---------------------------------------------------------------------------
// HTTP helpers with timing + error logging
// ---------------------------------------------------------------------------

async function apiRequest<T>(method: string, apiPath: string, body?: unknown): Promise<T> {
  const start = performance.now();
  const hasBody = method !== 'GET' && method !== 'DELETE';
  const headers = hasBody ? { 'Content-Type': 'application/json' } : undefined;

  try {
    const res = await fetch(buildUrl(apiPath), {
      method,
      headers,
      body: hasBody && body != null ? JSON.stringify(body) : undefined,
    });
    const ms = (performance.now() - start).toFixed(0);
    if (!res.ok) {
      const errBody = await res.json().catch(() => ({ error: res.statusText }));
      const msg = errBody.error || `HTTP ${res.status}`;
      queueError('error', `${method} ${apiPath} ${res.status} (${ms}ms)`, { error: msg });
      throw new HttpError(msg, res.status);
    }
    if (res.status !== 200) log.debug(`${method} ${apiPath} ${res.status} (${ms}ms)`);
    return res.json() as Promise<T>;
  } catch (err) {
    if (err instanceof Error && !(err instanceof HttpError)) {
      const ms = (performance.now() - start).toFixed(0);
      queueError('error', `${method} ${apiPath} failed (${ms}ms)`, { error: err.message });
    }
    throw err;
  }
}

export function apiGet<T>(apiPath: string): Promise<T> {
  return apiRequest<T>('GET', apiPath);
}

export function apiPost<T>(apiPath: string, body?: unknown): Promise<T> {
  return apiRequest<T>('POST', apiPath, body);
}

export function apiPatch<T>(apiPath: string, body: unknown): Promise<T> {
  return apiRequest<T>('PATCH', apiPath, body);
}

export function apiPut<T>(apiPath: string, body: unknown): Promise<T> {
  return apiRequest<T>('PUT', apiPath, body);
}

export function apiDel<T>(apiPath: string): Promise<T> {
  return apiRequest<T>('DELETE', apiPath);
}

// Tasks
export const fetchTasks = (includeArchived?: boolean) => {
  const qs = includeArchived ? '?include_archived=true' : '';
  return apiGet<TaskListResponse>(`/api/tasks${qs}`);
};
export interface AddTaskInput {
  title: string;
  project?: string;
  images?: File[];
}

export const parseTodos = (text: string, project?: string) =>
  apiPost<{ items: string[] }>('/api/ai/parse-todos', { text, project });

export async function addTask(input: AddTaskInput): Promise<TaskItem> {
  const form = new FormData();
  form.append('title', input.title);
  form.append('source', 'electron');
  if (input.project) form.append('project', input.project);
  if (input.images) {
    for (const img of input.images) {
      form.append('images', img, img.name);
    }
  }
  const res = await fetch(buildUrl('/api/tasks/add'), {
    method: 'POST',
    body: form,
  });
  if (!res.ok) {
    const errBody = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(errBody.error || `HTTP ${res.status}`);
  }
  return res.json() as Promise<TaskItem>;
}
export const deleteItems = (ids: number[], opts?: { close_pr?: boolean }) =>
  apiPost<{ ok: boolean; deleted: number; warnings?: string[] }>('/api/tasks/delete', {
    ids,
    ...opts,
  });
export const acceptItem = (id: number) => apiPost<void>('/api/tasks/accept', { id });
export const reopenItem = (id: number, feedback: string) =>
  apiPost<void>('/api/tasks/reopen', { id, feedback });
export const reworkItem = (id: number, feedback: string) =>
  apiPost<void>('/api/tasks/rework', { id, feedback });
export const fetchTimeline = (id: number) => apiGet<TimelineResponse>(`/api/tasks/${id}/timeline`);
export const fetchItemSessions = (id: number) =>
  apiGet<ItemSessionsResponse>(`/api/tasks/${id}/sessions`);

// Retry / Clarify
export const retryItem = (id: number) => apiPost<{ ok: boolean }>('/api/tasks/retry', { id });

export interface ClarifyResponse {
  ok: boolean;
  status: string;
  context?: string;
  questions?: {
    question: string;
    answer?: string | null;
    self_answered: boolean;
    category?: 'code' | 'intent';
  }[];
  session_id?: string;
  error?: string;
}

export const answerClarification = (id: number, answers: { question: string; answer: string }[]) =>
  apiPost<ClarifyResponse>(`/api/tasks/${id}/clarify`, { answers });

/** Flat-text answer for Telegram-style input */
export const answerClarificationText = (id: number, answer: string) =>
  apiPost<ClarifyResponse>(`/api/tasks/${id}/clarify`, { answer });

// Captain
export const triggerTick = (dryRun = false) =>
  apiPost<TickResult>('/api/captain/tick', { dry_run: dryRun });
export const nudgeWorker = (itemId: number, message: string) =>
  apiPost<{ worker?: string; pid?: number }>('/api/captain/nudge', {
    item_id: String(itemId),
    message,
  });
export const handoffItem = (id: number) => apiPost<{ ok: boolean }>('/api/tasks/handoff', { id });
export const cancelItem = (id: number) => apiPost<{ ok: boolean }>('/api/tasks/cancel', { id });

// Workers
export const fetchWorkers = () => apiGet<WorkersResponse>('/api/workers');

// Task Ask (multi-turn: first ask creates session, follow-ups resume)
export const askTask = (id: number, question: string) =>
  apiPost<AskResponse>('/api/tasks/ask', { id, question });

// End ask session
export const endAskSession = (id: number) =>
  apiPost<{ ok: boolean; ended: string }>('/api/tasks/ask/end', { id });

// Reopen from Q&A — synthesize conversation into reopen feedback
export const askReopen = (id: number) =>
  apiPost<{ ok: boolean; feedback: string }>('/api/tasks/ask/reopen', { id });

// Task Ask History
export const fetchAskHistory = (id: number) =>
  apiGet<AskHistoryResponse>(`/api/tasks/${id}/history`);

// Merge PR
export const mergePr = (prNumber: number, project: string) =>
  apiPost<{ ok: boolean; message: string }>('/api/tasks/merge', { pr_number: prNumber, project });

// PR Summary
export const fetchPrSummary = (id: number) =>
  apiGet<PrSummaryResponse>(`/api/tasks/${id}/pr-summary`);

// Sessions
export async function fetchSessions(page = 1, perPage = 50, category?: string) {
  const params = new URLSearchParams({ page: String(page), per_page: String(perPage) });
  if (category) params.set('category', category);
  return apiGet<SessionsResponse>(`/api/sessions?${params}`);
}
export const fetchTranscript = (sessionId: string) =>
  apiGet<TranscriptResponse>(`/api/sessions/${sessionId}/transcript`);

// SSE
export function connectSSE(
  onEvent: (event: { event: string; ts: number; data?: unknown }) => void,
  onStatusChange?: (status: SSEConnectionStatus) => void,
): EventSource {
  const source = new EventSource(buildUrl('/api/events'));

  // Deduplicate status changes — EventSource reconnect loops can flood setState
  let lastStatus: SSEConnectionStatus | null = null;
  const emitStatus = (status: SSEConnectionStatus) => {
    if (status === lastStatus) return;
    lastStatus = status;
    onStatusChange?.(status);
  };

  emitStatus('connecting');

  source.onopen = () => {
    emitStatus('connected');
  };

  let consecutiveParseFailures = 0;
  let degradedEmittedForStream = false;
  const PARSE_FAILURE_THRESHOLD = 5;

  source.onmessage = (msg) => {
    try {
      const data = JSON.parse(msg.data);
      consecutiveParseFailures = 0;
      onEvent(data);
    } catch (e) {
      consecutiveParseFailures++;
      log.warn('[SSE] failed to parse event data:', e);
      // First parse failure: emit degraded event so the DevInfoBar / indicator
      // reacts immediately. Keep the counter-based escalation for the toast.
      if (!degradedEmittedForStream && typeof window !== 'undefined') {
        degradedEmittedForStream = true;
        window.dispatchEvent(new CustomEvent(OBS_DEGRADED_EVENT));
      }
      if (consecutiveParseFailures === PARSE_FAILURE_THRESHOLD) {
        log.error(
          `[SSE] ${PARSE_FAILURE_THRESHOLD} consecutive parse failures, data stream may be corrupt`,
        );
        queueError(
          'error',
          `SSE stream degraded: ${PARSE_FAILURE_THRESHOLD} consecutive parse failures`,
        );
        emitStatus('disconnected');
      }
    }
  };

  source.onerror = () => {
    log.warn('[SSE] connection error — will auto-reconnect');
    emitStatus('disconnected');
  };

  // Named SSE events from the gateway: "snapshot_error" fires when the server
  // cannot build the initial snapshot (DB failure), and "resync" fires when
  // the broadcast stream lagged and the client needs to reload from REST to
  // catch up. EventSource does not route named events to onmessage, so
  // without explicit addEventListener calls these were silently dropped and
  // the UI would show stale data with no feedback.
  //
  // We deliberately use the name "snapshot_error" instead of "error" because
  // EventSource dispatches a plain Event (not a MessageEvent) to any listener
  // registered for "error" on native connection failures (network drops,
  // server restarts). That would make a named "error" event indistinguishable
  // from a reconnection attempt, producing spurious "snapshot failed" toasts.
  source.addEventListener('snapshot_error' as unknown as 'message', (msg: MessageEvent) => {
    try {
      const data = typeof msg.data === 'string' ? JSON.parse(msg.data) : null;
      const reason =
        data && typeof data === 'object' && 'error' in data
          ? String((data as { error: unknown }).error)
          : 'server failed to build snapshot';
      log.error('[SSE] snapshot_error event from server:', reason);
      queueError('error', `SSE snapshot failed: ${reason}`);
      emitStatus('disconnected');
    } catch (e) {
      log.error('[SSE] snapshot_error event (unparseable):', e);
      queueError('error', 'SSE snapshot failed (unparseable error payload)');
      emitStatus('disconnected');
    }
  });

  source.addEventListener('resync' as unknown as 'message', () => {
    log.warn('[SSE] broadcast lagged, client must reload snapshot');
    // Forward as a normal event so the DataProvider / store layer can
    // trigger a re-fetch of the initial data set.
    onEvent({ event: 'resync', ts: Date.now() });
  });

  return source;
}
