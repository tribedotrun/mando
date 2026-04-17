import type { SSEConnectionStatus } from '#renderer/global/types';
// eslint-disable-next-line no-restricted-imports -- logger is cross-cutting infrastructure
import log from '#renderer/global/service/logger';

function getErrorMessage(err: unknown, fallback: string): string {
  return err instanceof Error ? err.message : fallback;
}

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

// Error batching -- queues errors and POSTs to /api/client-logs every 5s

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

// SSE
export function connectSSE(
  onEvent: (event: { event: string; ts: number; data?: unknown }) => void,
  onStatusChange?: (status: SSEConnectionStatus) => void,
): EventSource {
  const source = new EventSource(buildUrl('/api/events'));

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

  // Named SSE events: "snapshot_error" (server DB failure), "resync" (stream lag).
  // Uses "snapshot_error" not "error" to avoid confusion with native EventSource errors.
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
