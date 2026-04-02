import type {
  TaskListResponse,
  TaskItem,
  HealthResponse,
  WorkersResponse,
  CronResponse,
  SessionsResponse,
  TranscriptResponse,
  TimelineResponse,
  TickResult,
  PrSummaryResponse,
  AskResponse,
  SSEConnectionStatus,
  ItemSessionsResponse,
  JournalResponse,
  PatternsResponse,
  DistillerResponse,
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
  deleteScoutItem,
  updateScoutStatus,
  bulkUpdateScout,
  bulkDeleteScout,
  askScout,
  actOnScoutItem,
  researchScout,
  publishScoutTelegraph,
  fetchScoutItemSessions,
} from '#renderer/api-scout';

export { postVoice, transcribeAudio } from '#renderer/api-voice';

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

const MAX_ERROR_BATCH = 200;
const MAX_FLUSH_RETRIES = 5;
const BASE_RETRY_MS = 5_000;
const MAX_RETRY_MS = 60_000;

function queueError(level: string, message: string, context?: unknown): void {
  if (errorBatch.length >= MAX_ERROR_BATCH) return;
  errorBatch.push({
    level,
    message,
    context,
    timestamp: new Date().toISOString(),
  });

  if (!batchTimer) {
    batchTimer = setTimeout(flushErrors, 5000);
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
    if (flushFailures >= MAX_FLUSH_RETRIES) {
      log.warn(
        `[obs] dropping ${entries.length} entries after ${MAX_FLUSH_RETRIES} flush failures`,
      );
      flushFailures = 0;
      return;
    }
    if (flushFailures === 1) {
      const reason = getErrorMessage(err, 'unknown');
      log.warn(`[obs] flush failed (${reason}), will retry`);
    }
    errorBatch.push(...entries.slice(0, MAX_ERROR_BATCH - errorBatch.length));
    if (!batchTimer && errorBatch.length > 0) {
      const delay = Math.min(BASE_RETRY_MS * 2 ** flushFailures, MAX_RETRY_MS);
      batchTimer = setTimeout(flushErrors, delay);
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

export function apiDel<T>(apiPath: string): Promise<T> {
  return apiRequest<T>('DELETE', apiPath);
}

// Health & system info (authenticated — includes config paths, projects, linear slug)
export const fetchHealth = () => apiGet<HealthResponse>('/api/health/system');

// Tasks
export const fetchTasks = (includeArchived?: boolean) => {
  const qs = includeArchived ? '?include_archived=true' : '';
  return apiGet<TaskListResponse>(`/api/tasks${qs}`);
};
export interface AddTaskInput {
  title: string;
  project?: string;
  context?: string;
  linearId?: string;
  plan?: string;
  noPr?: boolean;
  images?: File[];
}

export async function addTask(input: AddTaskInput): Promise<TaskItem> {
  const form = new FormData();
  form.append('title', input.title);
  if (input.project) form.append('project', input.project);
  if (input.context) form.append('context', input.context);
  if (input.linearId) form.append('linear_id', input.linearId);
  if (input.plan) form.append('plan', input.plan);
  if (input.noPr) form.append('no_pr', 'true');
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
export const deleteItems = (
  ids: number[],
  opts?: { close_pr?: boolean; cancel_linear?: boolean },
) =>
  apiPost<{ ok: boolean; deleted: number; warnings?: string[] }>('/api/tasks/delete', {
    ids,
    ...opts,
  });
export const updateItem = (id: number, fields: Partial<TaskItem>) =>
  apiPatch<TaskItem>(`/api/tasks/${id}`, fields);
export const bulkUpdate = (ids: number[], updates: Partial<TaskItem>) =>
  apiPost<void>('/api/tasks/bulk', { ids, updates });
export const acceptItem = (id: number) => apiPost<void>('/api/tasks/accept', { id });
export const reopenItem = (id: number, feedback: string) =>
  apiPost<void>('/api/tasks/reopen', { id, feedback });
export const reworkItem = (id: number, feedback: string) =>
  apiPost<void>('/api/tasks/rework', { id, feedback });
export const fetchTimeline = (id: number) => apiGet<TimelineResponse>(`/api/tasks/${id}/timeline`);
export const fetchItemSessions = (id: number) =>
  apiGet<ItemSessionsResponse>(`/api/tasks/${id}/sessions`);

// Archive / Unarchive
export const archiveItem = (id: number) => apiPost<{ ok: boolean }>(`/api/tasks/${id}/archive`);
export const unarchiveItem = (id: number) => apiPost<{ ok: boolean }>(`/api/tasks/${id}/unarchive`);

// Retry / Clarify
export const retryItem = (id: number) => apiPost<{ ok: boolean }>('/api/tasks/retry', { id });

export interface ClarifyResponse {
  ok: boolean;
  status: string;
  context?: string;
  questions?: string;
  session_id?: string;
  error?: string;
}
export const answerClarification = (id: number, answer: string) =>
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

// Workers
export const fetchWorkers = () => apiGet<WorkersResponse>('/api/workers');

// Cron
export const fetchCron = () => apiGet<CronResponse>('/api/cron');
export const addCronJob = (job: {
  name: string;
  schedule_kind: string;
  schedule_value: string;
  message: string;
}) => apiPost<void>('/api/cron/add', job);
export const removeCronJob = (id: string) => apiPost<void>('/api/cron/remove', { id });
export const runCronJob = (id: string) => apiPost<void>('/api/cron/run', { id });
export const toggleCronJob = (id: string, enabled: boolean) =>
  apiPost<void>('/api/cron/toggle', { id, enabled });

// Task Ask
export const askTask = (id: number, question: string) =>
  apiPost<AskResponse>('/api/tasks/ask', { id, question });

// Merge PR
export const mergePr = (pr: string, project: string) =>
  apiPost<{ ok: boolean; message: string }>('/api/tasks/merge', { pr, project });

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

// Captain memory / decision journal
export async function fetchJournal(params?: {
  worker?: string;
  limit?: number;
}): Promise<JournalResponse> {
  const qs = new URLSearchParams();
  if (params?.worker) qs.set('worker', params.worker);
  qs.set('limit', String(params?.limit ?? 50));
  return apiGet<JournalResponse>(`/api/journal?${qs}`);
}

export const fetchPatterns = (status?: string) => {
  const qs = status ? `?status=${status}` : '';
  return apiGet<PatternsResponse>(`/api/patterns${qs}`);
};

export const updatePatternStatus = (id: number, status: 'approved' | 'dismissed') =>
  apiPost<{ ok: boolean }>('/api/patterns/update', { id, status });

export const runDistiller = () => apiPost<DistillerResponse>('/api/knowledge/learn');

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
  const PARSE_FAILURE_THRESHOLD = 5;

  source.onmessage = (msg) => {
    try {
      const data = JSON.parse(msg.data);
      consecutiveParseFailures = 0;
      onEvent(data);
    } catch (e) {
      consecutiveParseFailures++;
      log.warn('[SSE] failed to parse event data:', e);
      if (consecutiveParseFailures === PARSE_FAILURE_THRESHOLD) {
        log.error(
          `[SSE] ${PARSE_FAILURE_THRESHOLD} consecutive parse failures — data stream may be corrupt`,
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

  return source;
}
