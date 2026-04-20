// Thrown by HTTP boundary funnels when a Zod schema rejects a response body.
// asResult() catches this before HttpError so consumers get code:'parse' with
// the full ZodIssue array rather than a synthetic code:'http' with status 0.

import type { ZodIssue } from 'zod';

/** Carries Zod issues from a schema-parse failure so asResult can produce code:'parse'. */
export class SchemaParseError extends Error {
  issues: ZodIssue[];
  where: string;
  constructor(issues: ZodIssue[], where: string) {
    super(`Schema parse failed: ${where}`);
    this.name = 'SchemaParseError';
    this.issues = issues;
    this.where = where;
  }
}
