import { z, type ZodType } from 'zod';
import { SchemaParseError } from '#result';
import {
  routes,
  type JsonRouteKey,
  type MultipartRouteKey,
  type NdjsonRouteKey,
  type Routes,
  type SseRouteKey,
  type StaticRouteKey,
} from './routes.ts';
import { bodySchemas, paramsSchemas, querySchemas } from './schemas.ts';

export type RouteKey = keyof Routes;
type Primitive = string | number | boolean;
type QueryValue = Primitive | null | undefined | Array<Primitive>;
type ParamValue = Primitive | null | undefined;

export type RouteParams<K extends RouteKey> = Routes[K] extends { params: infer P }
  ? P
  : Record<string, ParamValue>;

export type RouteQuery<K extends RouteKey> = Routes[K] extends { query: infer Q }
  ? Partial<Q>
  : Record<string, QueryValue>;

export type RouteBody<K extends RouteKey> = Routes[K] extends { body: infer B } ? B : never;
export type RouteRes<K extends RouteKey> = Routes[K] extends { res: infer R } ? R : unknown;
export type RouteEvent<K extends RouteKey> = Routes[K] extends { event: infer E } ? E : never;

export interface RouteResolveOptions<K extends RouteKey> {
  params?: RouteParams<K>;
  query?: RouteQuery<K>;
}

export type JsonRouteOptions<K extends JsonRouteKey> = RouteResolveOptions<K>;
export type MultipartRouteOptions<K extends MultipartRouteKey> = RouteResolveOptions<K>;
export type SseRouteOptions<K extends SseRouteKey> = RouteResolveOptions<K>;
export type StaticRouteOptions<K extends StaticRouteKey> = RouteResolveOptions<K>;
export type NdjsonRouteOptions<K extends NdjsonRouteKey> = RouteResolveOptions<K>;

type SchemaMap = Record<string, ZodType<unknown> | undefined>;

function parseIssues(message: string) {
  return [{ code: 'custom' as const, path: [], message }];
}

function isNonEmptyObject(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === 'object' && Object.keys(value).length > 0;
}

function stripUndefinedFields(raw: unknown): unknown {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return raw;
  return Object.fromEntries(
    Object.entries(raw as Record<string, unknown>).filter(([, value]) => value !== undefined),
  );
}

function validateRoutePart(
  key: RouteKey,
  kind: 'params' | 'query',
  raw: unknown,
  schemaMap: SchemaMap,
): Record<string, unknown> {
  const schema = schemaMap[key];
  const where = `route:${String(key)} ${kind}`;
  if (!schema) {
    if (isNonEmptyObject(raw)) {
      // invariant: route callers may only supply params/query declared by the shared contract
      throw new SchemaParseError(
        parseIssues(`${String(key)} does not declare ${kind} in the route contract`),
        where,
      );
    }
    return {};
  }
  const normalized = kind === 'query' ? stripUndefinedFields(raw ?? {}) : (raw ?? {});
  const parser = kind === 'query' && schema instanceof z.ZodObject ? schema.partial() : schema;
  const parsed = parser.safeParse(normalized);
  if (!parsed.success) {
    // invariant: route params/query must parse against the generated request schema before send
    throw new SchemaParseError(parsed.error.issues, where);
  }
  return parsed.data as Record<string, unknown>;
}

export function assertRouteBody(key: RouteKey, body: unknown): void {
  if (body instanceof FormData) {
    // Multipart FormData payloads are already serialized at the callsite; route
    // contracts still validate the object-shaped path when callers use JSON.
    return;
  }

  const schema = (bodySchemas as SchemaMap)[key];
  if (!schema) {
    if (body != null) {
      // invariant: route callers may only supply a body when the shared contract declares one
      throw new SchemaParseError(
        parseIssues(`${String(key)} does not declare a body in the route contract`),
        `route:${String(key)} body`,
      );
    }
    return;
  }

  const parsed = schema.safeParse(body ?? {});
  if (!parsed.success) {
    // invariant: outbound route bodies must parse against the generated request schema before send
    throw new SchemaParseError(parsed.error.issues, `route:${String(key)} body`);
  }
}

export function assertMultipartRouteBody<K extends RouteKey>(
  key: K,
  body: FormData | RouteBody<K>,
  shadowBody?: RouteBody<K>,
): void {
  if (body instanceof FormData) {
    if (shadowBody === undefined) {
      // invariant: multipart FormData routes must supply a schema-valid shadow body before send
      throw new SchemaParseError(
        parseIssues(
          'multipart FormData routes must provide a shadowBody for outbound schema preflight',
        ),
        `route:${String(key)} body`,
      );
    }
    assertRouteBody(key, shadowBody);
    return;
  }

  assertRouteBody(key, body);
}

export function resolveRoutePath<K extends RouteKey>(
  key: K,
  options?: RouteResolveOptions<K>,
): string {
  const route = routes[key];
  const validatedParams = validateRoutePart(
    key,
    'params',
    options?.params,
    paramsSchemas as SchemaMap,
  );
  const validatedQuery = validateRoutePart(key, 'query', options?.query, querySchemas as SchemaMap);
  const paramRecord = validatedParams as Record<string, ParamValue>;

  const withParams = route.path.replace(/\{([^}]+)\}/g, (_match, rawName: string) => {
    const value = paramRecord[rawName];
    if (value == null) {
      // invariant: route declares this path param so caller must always provide it
      throw new Error(`Missing route param "${rawName}" for ${String(key)}`);
    }
    return encodeURIComponent(String(value));
  });

  const params = new URLSearchParams();
  for (const [name, value] of Object.entries(validatedQuery as Record<string, QueryValue>)) {
    if (value == null) continue;
    if (Array.isArray(value)) {
      for (const item of value) {
        params.append(name, String(item));
      }
      continue;
    }
    params.set(name, String(value));
  }

  const query = params.toString();
  return query ? `${withParams}?${query}` : withParams;
}
