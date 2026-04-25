import { apiErrorMessage, parseTextWith } from '#result';
import { z, type ZodType } from 'zod';

const trimmedTextSchema = z.string().trim();
const nonEmptyTrimmedTextSchema = z.string().trim().min(1, 'Expected non-empty text');
const portTextSchema = z.string().trim().regex(/^\d+$/, 'Expected numeric port text');
const portNumberTextSchema = portTextSchema
  .transform((value) => Number(value))
  .refine(
    (value) => Number.isInteger(value) && value >= 1 && value <= 65535,
    'Expected port between 1 and 65535',
  );
const positiveIntegerTextSchema = z
  .string()
  .trim()
  .regex(/^[1-9]\d*$/, 'Expected positive integer text')
  .transform((value) => Number(value))
  .refine((value) => Number.isSafeInteger(value), 'Expected safe integer');
const launchctlPidTextSchema = z.string().transform((text, ctx) => {
  const match = text.match(/"PID"\s*=\s*(\d+)/);
  if (!match) return null;

  const pid = Number(match[1]);
  if (!Number.isSafeInteger(pid) || pid < 0) {
    ctx.addIssue({ code: z.ZodIssueCode.custom, message: 'Expected safe integer PID' });
    return z.NEVER;
  }
  return pid === 0 ? null : pid;
});

function unwrapOrThrow<T>(rawText: string, schema: ZodType<T>, where: string): T {
  const parsed = parseTextWith(rawText, schema, where);
  return parsed.match(
    (value) => value,
    (error) => {
      // invariant: mustParse* helpers escalate malformed boundary text to the owning caller
      throw new Error(apiErrorMessage(error));
    },
  );
}

export function mustParseTrimmedText(rawText: string, where: string): string {
  return unwrapOrThrow(rawText, trimmedTextSchema, where);
}

export function mustParseNonEmptyText(rawText: string, where: string): string {
  return unwrapOrThrow(rawText, nonEmptyTrimmedTextSchema, where);
}

export function mustParsePortText(rawText: string, where: string): string {
  return unwrapOrThrow(rawText, portTextSchema, where);
}

export function mustParsePortNumberText(rawText: string, where: string): number {
  return unwrapOrThrow(rawText, portNumberTextSchema, where);
}

export function mustParsePositiveIntegerText(rawText: string, where: string): number {
  return unwrapOrThrow(rawText, positiveIntegerTextSchema, where);
}

export function parseNonEmptyText(rawText: string, where: string): string | null {
  return parseTextWith(rawText, nonEmptyTrimmedTextSchema, where).match(
    (value) => value,
    () => null,
  );
}

export function parseLaunchctlPidText(rawText: string, where: string): number | null {
  return parseTextWith(rawText, launchctlPidTextSchema, where).match(
    (value) => value,
    () => null,
  );
}

export function hasNonEmptyText(rawText: string, where: string): boolean {
  return parseNonEmptyText(rawText, where) !== null;
}
