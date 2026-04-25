import { z } from 'zod';
import {
  type ApiError,
  type Result,
  err,
  ok,
  parseError as makeParseError,
  parseWith,
} from '#result';

const terminalOutputPayloadSchema = z
  .object({
    dataB64: z
      .string()
      .refine(
        (value) =>
          value.length === 0 ||
          /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(value),
        'Expected base64 payload',
      ),
  })
  .strict();

export function decodeTerminalOutputPayload(
  payload: unknown,
  where: string = 'sse:getTerminalByIdStream:output',
): Result<Uint8Array, ApiError> {
  return parseWith(terminalOutputPayloadSchema, payload, where).andThen(({ dataB64 }) => {
    try {
      const raw = atob(dataB64);
      const bytes = new Uint8Array(raw.length);
      for (let i = 0; i < raw.length; i++) {
        bytes[i] = raw.charCodeAt(i);
      }
      return ok(bytes);
    } catch {
      return err(
        makeParseError(
          [{ code: 'custom', message: 'Expected base64 payload', path: ['dataB64'] }],
          where,
          payload,
        ),
      );
    }
  });
}
