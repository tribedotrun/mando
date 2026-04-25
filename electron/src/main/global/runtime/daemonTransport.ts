import log from '#main/global/providers/logger';
import {
  assertRouteBody,
  resolveRoutePath,
  type JsonRouteOptions,
  type RouteBody,
  type RouteRes,
} from '#shared/daemon-contract/runtime';
import {
  routes as contractRoutes,
  type JsonRouteWithResKey,
  type Routes,
} from '#shared/daemon-contract/routes';
import { resSchemas } from '#shared/daemon-contract/schemas';
import type { ZodType } from 'zod';
import {
  type ApiError,
  type Result,
  type ResultAsync,
  err,
  fromPromise as resultFromPromise,
  httpError as makeHttpError,
  networkError as makeNetworkError,
  parseError as makeParseError,
  parseJsonText,
  SchemaParseError,
  timeoutError as makeTimeoutError,
  ok,
} from '#result';
import { parseNonEmptyText } from '#main/global/service/boundaryText';
import { readPort, readToken } from '#main/global/service/daemonDiscovery';

export type DaemonRouteFetchOptions<K extends JsonRouteWithResKey> = Omit<
  RequestInit,
  'body' | 'method'
> & {
  method?: Routes[K]['method'];
  body?: RouteBody<K> extends never ? never : RouteBody<K>;
};

async function daemonFetch(urlPath: string, options?: RequestInit) {
  const port = process.env.MANDO_GATEWAY_PORT || (await readPort());
  const token = process.env.MANDO_AUTH_TOKEN || (await readToken());
  const headers: Record<string, string> = {
    ...(options?.headers as Record<string, string>),
  };
  if (token) headers['Authorization'] = `Bearer ${token}`;
  if (!headers['Content-Type'] && options?.body) {
    headers['Content-Type'] = 'application/json';
  }
  return fetch(`http://127.0.0.1:${port}${urlPath}`, { ...options, headers });
}

async function responseTextOr(response: Response, fallback: string) {
  try {
    return await response.text();
  } catch {
    return fallback;
  }
}

export async function daemonRouteFetch<K extends JsonRouteWithResKey>(
  key: K,
  routeOptions?: JsonRouteOptions<K>,
  options?: DaemonRouteFetchOptions<K>,
) {
  const rawBody = options?.body as RouteBody<K> | undefined;
  const isFormData = rawBody instanceof FormData;
  assertRouteBody(key, rawBody);
  const body: BodyInit | undefined =
    rawBody == null ? undefined : isFormData ? rawBody : JSON.stringify(rawBody);
  return daemonFetch(resolveRoutePath(key, routeOptions), {
    ...options,
    body,
    method: options?.method ?? contractRoutes[key].method,
  });
}

export function daemonRouteJsonR<K extends JsonRouteWithResKey>(
  key: K,
  routeOptions?: JsonRouteOptions<K>,
  options?: DaemonRouteFetchOptions<K>,
): ResultAsync<RouteRes<K>, ApiError> {
  return resultFromPromise(daemonRouteFetch(key, routeOptions, options), (cause): ApiError => {
    if (cause instanceof Error && cause.name === 'AbortError') {
      return makeTimeoutError(0, `route:${String(key)}`);
    }
    if (cause instanceof SchemaParseError) {
      return makeParseError(cause.issues, cause.where ?? `route:${String(key)}`);
    }
    if (cause instanceof Error) return makeNetworkError(cause, `route:${String(key)}`);
    return makeNetworkError(String(cause), `route:${String(key)}`);
  }).andThen(async (response): Promise<Result<RouteRes<K>, ApiError>> => {
    if (!response.ok) {
      const detailText = await responseTextOr(response, response.statusText);
      const detail = parseNonEmptyText(detailText, `route:${String(key)} error`);
      return err(makeHttpError(response.status, detail || null, `HTTP ${response.status}`));
    }
    const rawText = await responseTextOr(response, '');
    const rawResult = parseJsonText(rawText, `daemon:${String(key)}`);
    if (rawResult.isErr()) {
      return err(makeParseError(rawResult.error.issues, rawResult.error.where, rawText));
    }
    const raw = rawResult.value;
    const schema = (resSchemas as Record<string, ZodType<unknown> | undefined>)[key as string];
    if (!schema) {
      log.warn(
        `[daemonRouteJsonR] no schema registered for route "${String(key)}" -- skipping validation (codegen gap)`,
      );
      return ok(raw as RouteRes<K>);
    }
    const parsed = schema.safeParse(raw);
    if (parsed.success) return ok(parsed.data as RouteRes<K>);
    log.warn(`daemonRouteJsonR schema parse failed for ${String(key)}`, parsed.error.issues);
    return err(makeParseError(parsed.error.issues, `daemon:${String(key)}`));
  });
}
