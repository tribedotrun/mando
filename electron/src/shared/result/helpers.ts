// Boundary helpers: turn raw external data into typed Results.
// Every helper takes a Zod schema and returns Result/ResultAsync.

import type { ZodType } from 'zod';

import {
  type ApiError,
  ApiErrorThrown,
  httpError,
  ioError,
  ipcError,
  networkError,
  parseError,
} from './errors.ts';
import { type Result, err, ok } from './result.ts';
import { ResultAsync, fromPromise } from './async-result.ts';

type ParseApiError = Extract<ApiError, { code: 'parse' }>;

function makeParseApiError(
  issues: ParseApiError['issues'],
  where: string,
  raw?: unknown,
): ParseApiError {
  return parseError(issues, where, raw) as ParseApiError;
}

// Parse arbitrary unknown input against a schema. The canonical boundary parser.
export function parseWith<T>(schema: ZodType<T>, raw: unknown, where: string): Result<T, ApiError> {
  const out = schema.safeParse(raw);
  if (out.success) return ok(out.data);
  return err(parseError(out.error.issues, where, raw));
}

// Parse a raw JSON string into unknown. JSON.parse failure becomes a typed parse error.
export function parseJsonText(rawText: string, where: string): Result<unknown, ParseApiError> {
  try {
    return ok<unknown, ParseApiError>(JSON.parse(rawText));
  } catch (cause) {
    return err(
      makeParseApiError([{ code: 'custom', path: [], message: String(cause) }], where, rawText),
    );
  }
}

// Parse a raw JSON string and immediately validate it against a schema.
export function parseJsonTextWith<T>(
  rawText: string,
  schema: ZodType<T>,
  where: string,
): Result<T, ParseApiError> {
  const parsedJson = parseJsonText(rawText, where);
  if (parsedJson.isErr()) return err<T, ParseApiError>(parsedJson.error);
  const out = schema.safeParse(parsedJson.value);
  if (out.success) return ok<T, ParseApiError>(out.data);
  return err<T, ParseApiError>(makeParseApiError(out.error.issues, where, parsedJson.value));
}

// Wrap a fetch Response: read body, parse error envelope on non-2xx, parse success body.
export function fromResponse<T>(
  responsePromise: Promise<Response>,
  schema: ZodType<T>,
  where: string,
): ResultAsync<T, ApiError> {
  return fromPromise(responsePromise, (cause) => networkError(cause, where)).andThen(
    async (res) => {
      if (!res.ok) {
        const body = await readJsonSafe(res);
        const message = extractErrorMessage(body) ?? `HTTP ${res.status}`;
        return err<T, ApiError>(httpError(res.status, body, message));
      }
      const body = await readJsonSafe(res);
      return parseWith(schema, body, where);
    },
  );
}

// Parse a single SSE message string. JSON.parse failure becomes a parse error.
export function fromSseMessage<T>(
  raw: string,
  schema: ZodType<T>,
  where: string,
): Result<T, ApiError> {
  return parseJsonText(raw, where).andThen((parsed) => parseWith(schema, parsed, where));
}

// Wrap an IPC invoke. Caller hands us a thenable; we map throws to ipc errors and parse the result.
export function fromIpc<T>(
  channel: string,
  invokePromise: Promise<unknown>,
  schema: ZodType<T>,
): ResultAsync<T, ApiError> {
  return fromPromise(invokePromise, (cause) => ipcError(channel, cause)).andThen((raw) =>
    parseWith(schema, raw, `ipc:${channel}`),
  );
}

// Read a file from disk, JSON.parse it, and validate against schema. Returns ResultAsync
// so file-not-found, JSON parse error, and schema mismatch are all typed errors.
export function fromFile<T>(
  readFile: () => Promise<string>,
  path: string,
  schema: ZodType<T>,
): ResultAsync<T, ApiError> {
  return fromPromise(readFile(), (cause) => ioError(path, cause)).andThen((text) =>
    parseJsonText(text, `file:${path}`).andThen((raw) => parseWith(schema, raw, `file:${path}`)),
  );
}

// React Query / library interop: translate Err into a thrown ApiErrorThrown.
// One-line escape hatch at queryFn / mutationFn boundaries. Lint allows throw in this helper.
export async function toReactQuery<T, E extends ApiError>(
  ra: ResultAsync<T, E> | Promise<Result<T, E>>,
): Promise<T> {
  const r = ra instanceof ResultAsync ? await ra.toPromise() : await ra;
  if (r.isOk()) return r.value;
  // invariant: toReactQuery is the documented Result→throw translator at library edges
  throw new ApiErrorThrown(r.error);
}

async function readJsonSafe(res: Response): Promise<unknown> {
  try {
    return await res.json();
  } catch {
    return null;
  }
}

function extractErrorMessage(body: unknown): string | undefined {
  if (body && typeof body === 'object' && 'error' in body) {
    const v = (body as { error: unknown }).error;
    if (typeof v === 'string') return v;
  }
  return undefined;
}
