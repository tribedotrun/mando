import {
  type ApiError,
  type ResultAsync,
  fromPromise as resultFromPromise,
  httpError as makeHttpError,
  networkError as makeNetworkError,
  parseError as makeParseError,
  SchemaParseError,
  timeoutError as makeTimeoutError,
} from '#result';
import type {
  DeleteJsonRouteWithResKey,
  GetJsonRouteWithResKey,
  PatchJsonRouteWithResKey,
  PostJsonRouteWithResKey,
  PostMultipartRouteWithResKey,
  PutJsonRouteWithResKey,
} from '#shared/daemon-contract/routes';
import type {
  JsonRouteOptions,
  MultipartRouteOptions,
  RouteBody,
  RouteRes,
} from '#shared/daemon-contract/runtime';
import {
  apiDeleteRoute,
  apiGetRoute,
  apiMultipartRoute,
  apiPatchRoute,
  apiPostRoute,
  apiPutRoute,
  type DeleteRouteOptions,
  HttpError,
} from '#renderer/global/providers/httpRoutes';

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
