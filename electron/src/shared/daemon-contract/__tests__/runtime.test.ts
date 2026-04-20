import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import { SchemaParseError } from '#result';
import { assertMultipartRouteBody, assertRouteBody, resolveRoutePath } from '../runtime.ts';

describe('daemon-contract runtime', () => {
  it('accepts an omitted EmptyRequest body', () => {
    assert.doesNotThrow(() => assertRouteBody('postCaptainStop', undefined));
  });

  it('rejects bodies for routes that do not declare one', () => {
    assert.throws(() => assertRouteBody('getHealth', {}), SchemaParseError);
  });

  it('rejects malformed outbound route bodies before send', () => {
    assert.throws(
      () =>
        assertRouteBody('postNotify', {
          owner: 123,
        }),
      SchemaParseError,
    );
  });

  it('rejects multipart FormData routes without a shadow body', () => {
    assert.throws(() => assertMultipartRouteBody('postTasksAsk', new FormData()), SchemaParseError);
  });

  it('accepts multipart FormData routes when the shadow body is valid', () => {
    assert.doesNotThrow(() =>
      assertMultipartRouteBody('postTasksAsk', new FormData(), { id: 1, question: 'hello' }),
    );
  });

  it('rejects undeclared query data', () => {
    assert.throws(
      () =>
        resolveRoutePath('getHealth', {
          query: { replay: 1 },
        }),
      SchemaParseError,
    );
  });

  it('rejects invalid declared query data', () => {
    assert.throws(
      () =>
        resolveRoutePath('getSessionsByIdMessages', {
          params: { id: 'session-1' },
          // @ts-expect-error deliberate contract violation for runtime guardrail coverage
          query: { limit: '10' },
        }),
      SchemaParseError,
    );
  });

  it('builds declared params and query into the request path', () => {
    assert.equal(
      resolveRoutePath('getSessionsByIdMessages', {
        params: { id: 'session-1' },
        query: { limit: 10, offset: 0 },
      }),
      '/api/sessions/session-1/messages?limit=10&offset=0',
    );
  });

  it('drops undefined query entries before schema preflight', () => {
    assert.equal(
      resolveRoutePath('getSessions', {
        query: { page: undefined, status: undefined } as unknown as { page: number | null },
      }),
      '/api/sessions',
    );
  });

  it('rejects when a declared path param is missing', () => {
    assert.throws(() => resolveRoutePath('getSessionsByIdMessages'), SchemaParseError);
  });
});
