// Raw daemon HTTP route helpers. ResultAsync wrappers live in httpResult.ts and
// the public renderer surface re-exports them from http.ts.

import {
  assertMultipartRouteBody,
  assertRouteBody,
  resolveRoutePath,
  type JsonRouteOptions,
  type MultipartRouteOptions,
  type RouteBody,
  type RouteKey,
  type RouteRes,
} from '#shared/daemon-contract/runtime';
import type {
  DeleteJsonRouteWithBodyKey,
  DeleteJsonRouteWithResKey,
  GetJsonRouteWithResKey,
  PatchJsonRouteWithResKey,
  PostJsonRouteWithResKey,
  PostMultipartRouteWithResKey,
  PutJsonRouteWithResKey,
} from '#shared/daemon-contract/routes';
import { resSchemas, errorResponseSchema } from '#shared/daemon-contract/schemas';
import type { ZodType } from 'zod';
import { parseJsonText, SchemaParseError } from '#result';
import { buildUrl } from '#renderer/global/providers/httpBase';
import { queueError } from '#renderer/global/providers/httpObsQueue';
// eslint-disable-next-line no-restricted-imports -- logger is cross-cutting infrastructure
import log from '#renderer/global/service/logger';

export class HttpError extends Error {
  status: number;

  constructor(message: string, status: number) {
    super(message);
    this.status = status;
  }
}

async function responseTextOrEmpty(response: Response): Promise<string> {
  try {
    return await response.text();
  } catch {
    return '';
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
      const rawErrText = await responseTextOrEmpty(response);
      const rawErrResult = parseJsonText(rawErrText, `route:${routeKey} error`);
      const rawErr = rawErrResult.isOk() ? rawErrResult.value : null;
      const parsed = errorResponseSchema.safeParse(rawErr);
      const message = parsed.success
        ? parsed.data.error
        : response.statusText || `HTTP ${response.status}`;
      queueError('error', `${method} ${apiPath} ${response.status} (${ms}ms)`, { error: message });
      throw new HttpError(message, response.status);
    }
    if (response.status !== 200) log.debug(`${method} ${apiPath} ${response.status} (${ms}ms)`);
    const rawText = await response.text();
    const rawResult = parseJsonText(rawText, `route:${routeKey} body`);
    if (rawResult.isErr()) {
      queueError('error', `${method} ${apiPath} response failed schema parse`, {
        route: routeKey,
        issues: rawResult.error.issues,
      });
      throw new SchemaParseError(rawResult.error.issues, rawResult.error.where);
    }
    const raw = rawResult.value;
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

export function apiGetRoute<K extends GetJsonRouteWithResKey>(
  key: K,
  options?: JsonRouteOptions<K>,
): Promise<RouteRes<K>> {
  return apiRequestInternal<RouteRes<K>>('GET', key, resolveRoutePath(key, options), undefined);
}

export function apiPostRoute<K extends PostJsonRouteWithResKey>(
  key: K,
  body?: RouteBody<K>,
  options?: JsonRouteOptions<K>,
): Promise<RouteRes<K>> {
  return apiRequestInternal<RouteRes<K>>('POST', key, resolveRoutePath(key, options), body);
}

export function apiPatchRoute<K extends PatchJsonRouteWithResKey>(
  key: K,
  body: RouteBody<K>,
  options?: JsonRouteOptions<K>,
): Promise<RouteRes<K>> {
  return apiRequestInternal<RouteRes<K>>('PATCH', key, resolveRoutePath(key, options), body);
}

export function apiPutRoute<K extends PutJsonRouteWithResKey>(
  key: K,
  body: RouteBody<K>,
  options?: JsonRouteOptions<K>,
): Promise<RouteRes<K>> {
  return apiRequestInternal<RouteRes<K>>('PUT', key, resolveRoutePath(key, options), body);
}

export type DeleteRouteOptions<K extends DeleteJsonRouteWithResKey> = JsonRouteOptions<K> &
  (K extends DeleteJsonRouteWithBodyKey ? { body: RouteBody<K> } : { body?: never });

export function apiDeleteRoute<K extends DeleteJsonRouteWithResKey>(
  key: K,
  options?: DeleteRouteOptions<K>,
): Promise<RouteRes<K>> {
  const deleteOptions = (options ?? {}) as JsonRouteOptions<K> & { body?: RouteBody<K> };
  const { body, ...routeOptions } = deleteOptions;
  return apiRequestInternal<RouteRes<K>>('DELETE', key, resolveRoutePath(key, routeOptions), body);
}

export async function apiMultipartRoute<K extends PostMultipartRouteWithResKey>(
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
    const rawErrText = await responseTextOrEmpty(response);
    const rawErrResult = parseJsonText(rawErrText, `route:${key as string} error`);
    const rawErr = rawErrResult.isOk() ? rawErrResult.value : null;
    const parsed = errorResponseSchema.safeParse(rawErr);
    const message = parsed.success
      ? parsed.data.error
      : response.statusText || `HTTP ${response.status}`;
    throw new HttpError(message, response.status);
  }
  const rawText = await response.text();
  const rawResult = parseJsonText(rawText, `route:${key as string} body`);
  if (rawResult.isErr()) {
    queueError('error', `POST multipart ${apiPath} response failed schema parse`, {
      route: key as string,
      issues: rawResult.error.issues,
    });
    throw new SchemaParseError(rawResult.error.issues, rawResult.error.where);
  }
  const raw = rawResult.value;
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
