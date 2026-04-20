// Pure SSE message-parse logic, extracted from http.ts so it is unit-testable
// without the renderer-side aliases. http.ts:connectSSE delegates to this and
// then handles the side effects (obs queue, status emission, degradation event).

import type { ZodType } from 'zod';

export interface SseParseResult<T> {
  /** Successfully parsed event data, or null when the message was rejected. */
  data: T | null;
  /** Reason the message was rejected; null on success. */
  failure: SseParseFailure | null;
}

export interface SseParseFailure {
  /** Always `'parse_failed'` — the obs event type emitted to VictoriaLogs. */
  event: 'parse_failed';
  /** Schema issues when the JSON parsed but the shape didn't match. */
  issues?: unknown;
  /** Raw cause message when JSON.parse itself threw. */
  cause?: string;
  /** Truncated raw payload for forensics (max 240 chars). */
  raw: string | null;
}

/**
 * Parse one SSE message. Returns `{ data, failure }` where exactly one is non-null.
 * Pure: no side effects. The caller is responsible for queuing the obs entry,
 * incrementing the consecutive-failures counter, and dispatching status events.
 */
export function parseSseMessage<T>(
  rawData: unknown,
  schema: ZodType<T> | undefined,
): SseParseResult<T> {
  if (typeof rawData !== 'string') {
    return {
      data: null,
      failure: {
        event: 'parse_failed',
        cause: 'event data is not a string',
        raw: null,
      },
    };
  }
  let json: unknown;
  try {
    json = JSON.parse(rawData);
  } catch (cause) {
    return {
      data: null,
      failure: {
        event: 'parse_failed',
        cause: cause instanceof Error ? cause.message : String(cause),
        raw: rawData.slice(0, 240),
      },
    };
  }
  if (!schema) {
    return { data: json as T, failure: null };
  }
  const parsed = schema.safeParse(json);
  if (parsed.success) {
    return { data: parsed.data, failure: null };
  }
  return {
    data: null,
    failure: {
      event: 'parse_failed',
      issues: parsed.error.issues,
      raw: rawData.slice(0, 240),
    },
  };
}
