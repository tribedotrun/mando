import type { SSEConnectionStatus, SSEEvent } from '#renderer/global/types';
// eslint-disable-next-line no-restricted-imports -- logger is cross-cutting infrastructure
import log from '#renderer/global/service/logger';
import {
  assertMultipartRouteBody,
  assertRouteBody,
  resolveRoutePath,
  type JsonRouteOptions,
  type MultipartRouteOptions,
  type RouteBody,
  type RouteEvent,
  type RouteKey,
  type RouteRes,
  type SseRouteOptions,
} from '#shared/daemon-contract/runtime';
import type {
  DeleteJsonRouteWithBodyKey,
  DeleteJsonRouteWithResKey,
  GetJsonRouteWithResKey,
  GetSseRouteWithEventKey,
  PatchJsonRouteWithResKey,
  PostJsonRouteWithResKey,
  PostMultipartRouteWithResKey,
  PutJsonRouteWithResKey,
} from '#shared/daemon-contract/routes';
import { resSchemas, eventSchemas, errorResponseSchema } from '#shared/daemon-contract/schemas';
import type { ZodType } from 'zod';
import {
  type ApiError,
  type ResultAsync,
  fromPromise as resultFromPromise,
  httpError as makeHttpError,
  networkError as makeNetworkError,
  parseError as makeParseError,
  SchemaParseError,
  timeoutError as makeTimeoutError,
  parseSseMessage,
} from '#result';
import { buildUrl, initBaseUrl, staticRoutePath } from '#renderer/global/providers/httpBase';
import {
  __testClearErrorBatch,
  __testGetErrorBatch,
  queueError,
} from '#renderer/global/providers/httpObsQueue';
import { reportObsDegraded } from '#renderer/global/providers/obsHealth';

export { buildUrl, initBaseUrl, staticRoutePath, __testGetErrorBatch, __testClearErrorBatch };

class HttpError extends Error {
  status: number;

  constructor(message: string, status: number) {
    super(message);
    this.status = status;
  }
}

async function apiRequestInternal<T>(
  method: string,
  routeKey: RouteKey,
  apiPath: string,
  body: unknown,
): Promise<T> {
  assertRouteBody(routeKey, body);
  const start = performance.now();
  const hasBody = method !== 'GET' && body != null;
  const headers = hasBody ? { 'Content-Type': 'application/json' } : undefined;
  const schema = (resSchemas as Partial<Record<RouteKey, ZodType<unknown> | undefined>>)[routeKey];

  try {
    const response = await fetch(buildUrl(apiPath), {
      method,
      headers,
      body: hasBody && body != null ? JSON.stringify(body) : undefined,
    });
    const ms = (performance.now() - start).toFixed(0);
    if (!response.ok) {
      const rawErr: unknown = await response.json().catch(() => null);
      const parsed = errorResponseSchema.safeParse(rawErr);
      const message = parsed.success
        ? parsed.data.error
        : response.statusText || `HTTP ${response.status}`;
      queueError('error', `${method} ${apiPath} ${response.status} (${ms}ms)`, { error: message });
      throw new HttpError(message, response.status);
    }
    if (response.status !== 200) log.debug(`${method} ${apiPath} ${response.status} (${ms}ms)`);
    const raw: unknown = await response.json();
    if (!schema) {
      log.warn(`[http] no schema registered for route "${routeKey}" -- skipping validation`);
      return raw as T;
    }
    const parsed = schema.safeParse(raw);
    if (parsed.success) return parsed.data as T;
    queueError('error', `${method} ${apiPath} response failed schema parse`, {
      route: routeKey,
      issues: parsed.error.issues,
    });
    throw new SchemaParseError(parsed.error.issues, `route:${routeKey} body`);
  } catch (error) {
    if (
      error instanceof Error &&
      !(error instanceof HttpError) &&
      !(error instanceof SchemaParseError)
    ) {
      const ms = (performance.now() - start).toFixed(0);
      queueError('error', `${method} ${apiPath} failed (${ms}ms)`, { error: error.message });
    }
    throw error;
  }
}

function apiGetRoute<K extends GetJsonRouteWithResKey>(
  key: K,
  options?: JsonRouteOptions<K>,
): Promise<RouteRes<K>> {
  return apiRequestInternal<RouteRes<K>>('GET', key, resolveRoutePath(key, options), undefined);
}

function apiPostRoute<K extends PostJsonRouteWithResKey>(
  key: K,
  body?: RouteBody<K>,
  options?: JsonRouteOptions<K>,
): Promise<RouteRes<K>> {
  return apiRequestInternal<RouteRes<K>>('POST', key, resolveRoutePath(key, options), body);
}

function apiPatchRoute<K extends PatchJsonRouteWithResKey>(
  key: K,
  body: RouteBody<K>,
  options?: JsonRouteOptions<K>,
): Promise<RouteRes<K>> {
  return apiRequestInternal<RouteRes<K>>('PATCH', key, resolveRoutePath(key, options), body);
}

function apiPutRoute<K extends PutJsonRouteWithResKey>(
  key: K,
  body: RouteBody<K>,
  options?: JsonRouteOptions<K>,
): Promise<RouteRes<K>> {
  return apiRequestInternal<RouteRes<K>>('PUT', key, resolveRoutePath(key, options), body);
}

type DeleteRouteOptions<K extends DeleteJsonRouteWithResKey> = JsonRouteOptions<K> &
  (K extends DeleteJsonRouteWithBodyKey ? { body: RouteBody<K> } : { body?: never });

function apiDeleteRoute<K extends DeleteJsonRouteWithResKey>(
  key: K,
  options?: DeleteRouteOptions<K>,
): Promise<RouteRes<K>> {
  const deleteOptions = (options ?? {}) as JsonRouteOptions<K> & { body?: RouteBody<K> };
  const { body, ...routeOptions } = deleteOptions;
  return apiRequestInternal<RouteRes<K>>('DELETE', key, resolveRoutePath(key, routeOptions), body);
}

async function apiMultipartRoute<K extends PostMultipartRouteWithResKey>(
  key: K,
  body: FormData | RouteBody<K>,
  options?: MultipartRouteOptions<K>,
  shadowBody?: RouteBody<K>,
): Promise<RouteRes<K>> {
  const isForm = body instanceof FormData;
  const apiPath = resolveRoutePath(key, options);
  const schema = (resSchemas as Record<string, ZodType<unknown> | undefined>)[key as string];
  try {
    assertMultipartRouteBody(key, body, shadowBody);
  } catch (error) {
    if (isForm && error instanceof SchemaParseError) {
      queueError('error', `POST multipart ${apiPath} missing shadow body`, {
        route: key as string,
      });
    }
    throw error;
  }
  const response = await fetch(buildUrl(apiPath), {
    method: 'POST',
    headers: isForm ? undefined : { 'Content-Type': 'application/json' },
    body: isForm ? body : JSON.stringify(body),
  });
  if (!response.ok) {
    const rawErr: unknown = await response.json().catch(() => null);
    const parsed = errorResponseSchema.safeParse(rawErr);
    const message = parsed.success
      ? parsed.data.error
      : response.statusText || `HTTP ${response.status}`;
    throw new HttpError(message, response.status);
  }
  const raw: unknown = await response.json();
  if (!schema) {
    log.warn(`[http] no schema registered for route "${key as string}" -- skipping validation`);
    return raw as RouteRes<K>;
  }
  const parsed = schema.safeParse(raw);
  if (parsed.success) return parsed.data as RouteRes<K>;
  queueError('error', `POST multipart ${apiPath} response failed schema parse`, {
    route: key as string,
    issues: parsed.error.issues,
  });
  throw new SchemaParseError(parsed.error.issues, `route:${key as string} body`);
}

export function openSseRoute<K extends GetSseRouteWithEventKey>(
  key: K,
  options?: SseRouteOptions<K>,
): EventSource {
  return new EventSource(buildUrl(resolveRoutePath(key, options)));
}

function asResult<K extends string>(
  fn: () => Promise<unknown>,
  key: K,
): ResultAsync<unknown, ApiError> {
  return resultFromPromise(fn(), (cause): ApiError => {
    if (cause instanceof Error && cause.name === 'AbortError') {
      return makeTimeoutError(0, `route:${String(key)}`);
    }
    if (cause instanceof SchemaParseError) {
      return makeParseError(cause.issues, cause.where ?? `route:${String(key)}`);
    }
    if (cause instanceof HttpError) {
      return makeHttpError(cause.status, null, cause.message);
    }
    if (cause instanceof Error) {
      return makeNetworkError(cause, `route:${String(key)}`);
    }
    return makeNetworkError(String(cause), `route:${String(key)}`);
  });
}

export function apiGetRouteR<K extends GetJsonRouteWithResKey>(
  key: K,
  options?: JsonRouteOptions<K>,
): ResultAsync<RouteRes<K>, ApiError> {
  return asResult(() => apiGetRoute(key, options), key) as ResultAsync<RouteRes<K>, ApiError>;
}

export function apiPostRouteR<K extends PostJsonRouteWithResKey>(
  key: K,
  body?: RouteBody<K>,
  options?: JsonRouteOptions<K>,
): ResultAsync<RouteRes<K>, ApiError> {
  return asResult(() => apiPostRoute(key, body, options), key) as ResultAsync<
    RouteRes<K>,
    ApiError
  >;
}

export function apiPatchRouteR<K extends PatchJsonRouteWithResKey>(
  key: K,
  body: RouteBody<K>,
  options?: JsonRouteOptions<K>,
): ResultAsync<RouteRes<K>, ApiError> {
  return asResult(() => apiPatchRoute(key, body, options), key) as ResultAsync<
    RouteRes<K>,
    ApiError
  >;
}

export function apiPutRouteR<K extends PutJsonRouteWithResKey>(
  key: K,
  body: RouteBody<K>,
  options?: JsonRouteOptions<K>,
): ResultAsync<RouteRes<K>, ApiError> {
  return asResult(() => apiPutRoute(key, body, options), key) as ResultAsync<RouteRes<K>, ApiError>;
}

export function apiDeleteRouteR<K extends DeleteJsonRouteWithResKey>(
  key: K,
  options?: DeleteRouteOptions<K>,
): ResultAsync<RouteRes<K>, ApiError> {
  return asResult(() => apiDeleteRoute(key, options), key) as ResultAsync<RouteRes<K>, ApiError>;
}

export function apiMultipartRouteR<K extends PostMultipartRouteWithResKey>(
  key: K,
  body: FormData | RouteBody<K>,
  options?: MultipartRouteOptions<K>,
  shadowBody?: RouteBody<K>,
): ResultAsync<RouteRes<K>, ApiError> {
  return asResult(() => apiMultipartRoute(key, body, options, shadowBody), key) as ResultAsync<
    RouteRes<K>,
    ApiError
  >;
}

export function connectSSE(
  onEvent: (event: SSEEvent) => void,
  onStatusChange?: (status: SSEConnectionStatus) => void,
): EventSource {
  const source = openSseRoute('getEvents');

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
  const sseSchema = (eventSchemas as Record<string, ZodType<unknown> | undefined>).getEvents;

  source.onmessage = (message) => {
    const result = parseSseMessage(message.data as unknown, sseSchema);
    if (result.failure) {
      consecutiveParseFailures++;
      queueError('error', 'SSE parse_failed', result.failure);
      if (!degradedEmittedForStream) {
        degradedEmittedForStream = true;
        reportObsDegraded();
      }
      if (consecutiveParseFailures === 5) {
        log.error('[SSE] 5 consecutive parse failures, data stream may be corrupt');
        emitStatus('disconnected');
      }
      return;
    }

    consecutiveParseFailures = 0;
    const data = result.data as RouteEvent<'getEvents'>;

    if (data.event === 'snapshot_error') {
      const payload = data.data.data;
      const reason =
        payload && typeof payload === 'object' && 'message' in payload
          ? String((payload as { message: unknown }).message)
          : 'server failed to build snapshot';
      log.error('[SSE] snapshot_error event from server:', reason);
      queueError('error', `SSE snapshot failed: ${reason}`);
      emitStatus('disconnected');
    } else if (data.event === 'resync') {
      log.warn('[SSE] broadcast lagged, client must reload snapshot');
    }

    onEvent(data);
  };

  source.onerror = () => {
    log.warn('[SSE] connection error — will auto-reconnect');
    emitStatus('disconnected');
  };

  return source;
}
