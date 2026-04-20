// ApiError discriminated union: every expected failure mode that can cross a boundary.
// Programmer bugs (invariant violations) throw instead of producing an ApiError.

import type { ZodIssue } from 'zod';

export type ApiError =
  | { code: 'http'; status: number; body: unknown; message: string }
  | { code: 'parse'; issues: ZodIssue[]; raw?: unknown; where: string }
  | { code: 'network'; cause: string; where: string }
  | { code: 'timeout'; ms: number; where: string }
  | { code: 'ipc'; channel: string; cause: string }
  | { code: 'io'; path: string; cause: string }
  | { code: 'invariant'; message: string };

export function httpError(status: number, body: unknown, message: string): ApiError {
  return { code: 'http', status, body, message };
}

export function parseError(issues: ZodIssue[], where: string, raw?: unknown): ApiError {
  return { code: 'parse', issues, where, raw };
}

export function networkError(cause: unknown, where: string): ApiError {
  return { code: 'network', cause: errorMessage(cause), where };
}

export function timeoutError(ms: number, where: string): ApiError {
  return { code: 'timeout', ms, where };
}

export function ipcError(channel: string, cause: unknown): ApiError {
  return { code: 'ipc', channel, cause: errorMessage(cause) };
}

export function ioError(path: string, cause: unknown): ApiError {
  return { code: 'io', path, cause: errorMessage(cause) };
}

export function invariantError(message: string): ApiError {
  return { code: 'invariant', message };
}

export function apiErrorMessage(err: ApiError): string {
  switch (err.code) {
    case 'http':
      return err.message || `HTTP ${err.status}`;
    case 'parse':
      return `Parse failed at ${err.where}: ${err.issues.map((i) => i.message).join('; ')}`;
    case 'network':
      return `Network error at ${err.where}: ${err.cause}`;
    case 'timeout':
      return `Timed out after ${err.ms}ms at ${err.where}`;
    case 'ipc':
      return `IPC ${err.channel} failed: ${err.cause}`;
    case 'io':
      return `IO error at ${err.path}: ${err.cause}`;
    case 'invariant':
      return `Invariant violated: ${err.message}`;
  }
}

function errorMessage(cause: unknown): string {
  if (cause instanceof Error) return cause.message;
  if (typeof cause === 'string') return cause;
  return String(cause);
}

// Carries an ApiError as a thrown value at React-Query / library interop edges.
// The toReactQuery() helper produces these; nothing else should construct them.
export class ApiErrorThrown extends Error {
  readonly apiError: ApiError;
  constructor(err: ApiError) {
    super(apiErrorMessage(err));
    this.name = 'ApiErrorThrown';
    this.apiError = err;
  }
}
