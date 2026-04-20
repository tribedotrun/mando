import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import {
  ResultAsync,
  combineAsync,
  errAsync,
  fromPromise,
  fromSafePromise,
  fromThrowable,
  okAsync,
} from '../async-result.ts';
import { err, ok } from '../result.ts';

describe('ResultAsync', () => {
  it('okAsync resolves to Ok', async () => {
    const r = await okAsync<number, string>(5);
    assert.equal(r.unwrap(), 5);
  });

  it('errAsync resolves to Err', async () => {
    const r = await errAsync<number, string>('boom');
    assert.equal(r.unwrapOr(0), 0);
  });

  it('fromPromise wraps successful promise', async () => {
    const r = await fromPromise(Promise.resolve(42), () => 'unused');
    assert.equal(r.unwrap(), 42);
  });

  it('fromPromise wraps rejected promise', async () => {
    const r = await fromPromise(Promise.reject(new Error('boom')), (e) => (e as Error).message);
    assert.equal(r.isErr(), true);
    assert.equal(r.unwrapOr(''), '');
  });

  it('fromSafePromise never errors', async () => {
    const r = await fromSafePromise(Promise.resolve('hi'));
    assert.equal(r.unwrap(), 'hi');
  });

  it('andThen chains async', async () => {
    const r = await okAsync<number, string>(2).andThen((n) => okAsync<number, string>(n + 1));
    assert.equal(r.unwrap(), 3);
  });

  it('andThen accepts sync Result', async () => {
    const r = await okAsync<number, string>(2).andThen((n) => ok<number, string>(n + 10));
    assert.equal(r.unwrap(), 12);
  });

  it('andThen short-circuits on err', async () => {
    let called = false;
    const r = await errAsync<number, string>('boom').andThen((n) => {
      called = true;
      return okAsync<number, string>(n + 1);
    });
    assert.equal(r.isErr(), true);
    assert.equal(called, false);
  });

  it('map transforms ok with async fn', async () => {
    const r = await okAsync<number, string>(2).map(async (n) => n * 5);
    assert.equal(r.unwrap(), 10);
  });

  it('mapErr transforms err', async () => {
    const r = await errAsync<number, string>('boom').mapErr((e) => e.toUpperCase());
    assert.equal(r.isErr(), true);
  });

  it('orElse recovers async', async () => {
    const r = await errAsync<number, string>('boom').orElse(() => okAsync<number, string>(99));
    assert.equal(r.unwrap(), 99);
  });

  it('match resolves to value', async () => {
    const out = await okAsync<number, string>(7).match(
      (v) => `ok:${v}`,
      () => 'err',
    );
    assert.equal(out, 'ok:7');
  });

  it('toPromise returns the inner Promise<Result>', async () => {
    const p = okAsync<number, string>(1).toPromise();
    assert.ok(p instanceof Promise);
    const r = await p;
    assert.equal(r.unwrap(), 1);
  });

  it('unwrapOr resolves to fallback on err', async () => {
    const out = await errAsync<number, string>('boom').unwrapOr(7);
    assert.equal(out, 7);
  });

  it('fromThrowable wraps a throwing sync fn', () => {
    const wrapped = fromThrowable(
      (n: number) => {
        if (n < 0) throw new Error('neg');
        return n * 2;
      },
      (e) => (e as Error).message,
    );
    assert.equal(wrapped(3).unwrap(), 6);
    assert.equal(wrapped(-1).unwrapOr(0), 0);
  });

  it('combineAsync resolves all ok', async () => {
    const r = await combineAsync<number, string>([okAsync(1), okAsync(2), okAsync(3)]);
    assert.deepEqual(r.unwrap(), [1, 2, 3]);
  });

  it('combineAsync short-circuits on first err', async () => {
    const r = await combineAsync<number, string>([okAsync(1), errAsync('boom'), okAsync(3)]);
    assert.equal(r.isErr(), true);
  });

  it('is thenable: await unwraps to Result', async () => {
    const ra = okAsync<number, string>(7);
    const r = await ra;
    assert.equal(r.unwrap(), 7);
  });

  it('errAsync containing rejected promise carries err', async () => {
    const ra = new ResultAsync<number, string>(Promise.resolve(err('boom')));
    const r = await ra;
    assert.equal(r.isErr(), true);
  });
});
