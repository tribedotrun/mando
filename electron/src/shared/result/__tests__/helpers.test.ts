import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { z } from 'zod';

import { ApiErrorThrown } from '../errors.ts';
import {
  fromIpc,
  fromResponse,
  fromSseMessage,
  parseJsonText,
  parseJsonTextWith,
  parseWith,
  toReactQuery,
} from '../helpers.ts';
import { errAsync, okAsync } from '../async-result.ts';

const taskSchema = z.object({ id: z.number(), title: z.string() });

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

describe('parseWith', () => {
  it('returns Ok on valid input', () => {
    const r = parseWith(taskSchema, { id: 1, title: 'x' }, 'test');
    assert.equal(r.unwrap().id, 1);
  });

  it('returns Err with parse code on invalid', () => {
    const r = parseWith(taskSchema, { id: 'one' }, 'test');
    assert.equal(r.isErr(), true);
    r.match(
      () => assert.fail('expected err'),
      (e) => {
        assert.equal(e.code, 'parse');
        if (e.code === 'parse') assert.ok(e.issues.length > 0);
      },
    );
  });
});

describe('parseJsonText', () => {
  it('returns Ok on valid JSON text', () => {
    const r = parseJsonText('{"id":1}', 'json:test');
    assert.deepEqual(r.unwrap(), { id: 1 });
  });

  it('returns Err on malformed JSON text', () => {
    const r = parseJsonText('{broken', 'json:test');
    r.match(
      () => assert.fail('expected err'),
      (e) => assert.equal(e.code, 'parse'),
    );
  });
});

describe('parseJsonTextWith', () => {
  it('returns Ok when JSON text matches the schema', () => {
    const r = parseJsonTextWith('{"id":1,"title":"x"}', taskSchema, 'json-schema:test');
    assert.equal(r.unwrap().title, 'x');
  });

  it('returns Err when JSON text fails the schema', () => {
    const r = parseJsonTextWith('{"id":"bad"}', taskSchema, 'json-schema:test');
    r.match(
      () => assert.fail('expected err'),
      (e) => assert.equal(e.code, 'parse'),
    );
  });
});

describe('fromResponse', () => {
  it('parses successful body', async () => {
    const r = await fromResponse(
      Promise.resolve(jsonResponse({ id: 1, title: 'x' })),
      taskSchema,
      'test',
    );
    assert.equal(r.unwrap().id, 1);
  });

  it('reports HTTP error envelope on non-2xx', async () => {
    const r = await fromResponse(
      Promise.resolve(jsonResponse({ error: 'boom' }, 500)),
      taskSchema,
      'test',
    );
    r.match(
      () => assert.fail('expected err'),
      (e) => {
        assert.equal(e.code, 'http');
        if (e.code === 'http') {
          assert.equal(e.status, 500);
          assert.equal(e.message, 'boom');
        }
      },
    );
  });

  it('reports parse error on malformed success body', async () => {
    const r = await fromResponse(Promise.resolve(jsonResponse({ wrong: 1 })), taskSchema, 'test');
    r.match(
      () => assert.fail('expected err'),
      (e) => assert.equal(e.code, 'parse'),
    );
  });

  it('reports network error when fetch rejects', async () => {
    const r = await fromResponse(Promise.reject(new Error('socket hang up')), taskSchema, 'test');
    r.match(
      () => assert.fail('expected err'),
      (e) => assert.equal(e.code, 'network'),
    );
  });
});

describe('fromSseMessage', () => {
  it('parses valid SSE payload', () => {
    const r = fromSseMessage('{"id":1,"title":"x"}', taskSchema, 'sse:test');
    assert.equal(r.unwrap().id, 1);
  });

  it('reports parse on bad JSON', () => {
    const r = fromSseMessage('not json', taskSchema, 'sse:test');
    r.match(
      () => assert.fail('expected err'),
      (e) => assert.equal(e.code, 'parse'),
    );
  });

  it('reports parse on schema mismatch', () => {
    const r = fromSseMessage('{"wrong":1}', taskSchema, 'sse:test');
    r.match(
      () => assert.fail('expected err'),
      (e) => assert.equal(e.code, 'parse'),
    );
  });
});

describe('fromIpc', () => {
  it('parses successful IPC return', async () => {
    const r = await fromIpc('test-channel', Promise.resolve({ id: 1, title: 'x' }), taskSchema);
    assert.equal(r.unwrap().id, 1);
  });

  it('reports ipc error on rejection', async () => {
    const r = await fromIpc('test-channel', Promise.reject(new Error('boom')), taskSchema);
    r.match(
      () => assert.fail('expected err'),
      (e) => {
        assert.equal(e.code, 'ipc');
        if (e.code === 'ipc') assert.equal(e.channel, 'test-channel');
      },
    );
  });

  it('reports parse error on bad shape', async () => {
    const r = await fromIpc('test-channel', Promise.resolve({ wrong: 1 }), taskSchema);
    r.match(
      () => assert.fail('expected err'),
      (e) => assert.equal(e.code, 'parse'),
    );
  });
});

describe('toReactQuery', () => {
  it('returns value on Ok', async () => {
    const v = await toReactQuery(okAsync<number, never>(7));
    assert.equal(v, 7);
  });

  it('throws ApiErrorThrown on Err', async () => {
    await assert.rejects(
      () => toReactQuery(errAsync(parseErr())),
      (e: unknown) => {
        assert.ok(e instanceof ApiErrorThrown);
        return true;
      },
    );
  });
});

function parseErr() {
  const r = parseWith(taskSchema, {}, 'test');
  if (r.isOk()) throw new Error('test setup: expected parse to fail');
  return r.error;
}
