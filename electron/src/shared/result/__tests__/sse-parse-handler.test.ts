// Tests the pure SSE parse-and-dispatch logic that connectSSE delegates to.
// connectSSE in renderer/global/providers/http.ts wraps these results with the
// obs queue + status emission, so by testing parseSseMessage we prove the
// production code path's failure detection (the side-effect wiring is then
// trivial application of these results).

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { z } from 'zod';

import { parseSseMessage } from '../sse-parse-handler.ts';

const envelopeSchema = z.object({
  event: z.enum(['tasks', 'sessions']),
  data: z.object({ ts: z.number(), data: z.unknown().nullable() }),
});

describe('parseSseMessage (live SSE handler logic)', () => {
  it('returns data on valid string + valid schema', () => {
    const r = parseSseMessage(
      JSON.stringify({ event: 'tasks', data: { ts: 1, data: null } }),
      envelopeSchema,
    );
    assert.equal(r.failure, null);
    assert.deepEqual(r.data, { event: 'tasks', data: { ts: 1, data: null } });
  });

  it('returns parse_failed when raw data is not a string', () => {
    const r = parseSseMessage(42, envelopeSchema);
    assert.equal(r.data, null);
    assert.equal(r.failure?.event, 'parse_failed');
    assert.match(r.failure?.cause ?? '', /not a string/);
    assert.equal(r.failure?.raw, null);
  });

  it('returns parse_failed when JSON.parse throws', () => {
    const r = parseSseMessage('not valid {{{', envelopeSchema);
    assert.equal(r.data, null);
    assert.equal(r.failure?.event, 'parse_failed');
    assert.ok(r.failure?.cause);
    assert.equal(r.failure?.raw, 'not valid {{{');
  });

  it('returns parse_failed with issues when schema mismatches', () => {
    const r = parseSseMessage(JSON.stringify({ event: 'unknown_kind' }), envelopeSchema);
    assert.equal(r.data, null);
    assert.equal(r.failure?.event, 'parse_failed');
    assert.ok(Array.isArray(r.failure?.issues));
    assert.equal(r.failure?.raw, '{"event":"unknown_kind"}');
  });

  it('passes raw json through unchanged when no schema is registered', () => {
    const r = parseSseMessage(JSON.stringify({ arbitrary: 'shape' }), undefined);
    assert.equal(r.failure, null);
    assert.deepEqual(r.data, { arbitrary: 'shape' });
  });

  it('truncates raw to 240 chars in failure record (forensics safety)', () => {
    const long = '"' + 'x'.repeat(500) + '"';
    const r = parseSseMessage(long, envelopeSchema);
    assert.equal(r.failure?.event, 'parse_failed');
    assert.ok((r.failure?.raw ?? '').length <= 240);
  });
});
