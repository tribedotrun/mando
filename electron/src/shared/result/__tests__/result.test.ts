import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import { Err, combine, err, isErr, isOk, ok } from '../result.ts';

describe('Result', () => {
  it('ok.unwrap returns value', () => {
    const r = ok<number, string>(42);
    assert.equal(r.unwrap(), 42);
    assert.equal(r.isOk(), true);
    assert.equal(r.isErr(), false);
    assert.equal(isOk(r), true);
  });

  it('err.unwrap throws', () => {
    const r = err<number, string>('boom');
    assert.throws(() => r.unwrap(), /boom/);
    assert.equal(r.unwrapOr(7), 7);
    assert.equal(isErr(r), true);
  });

  it('map transforms ok, leaves err', () => {
    assert.equal(
      ok<number, string>(2)
        .map((n) => n * 3)
        .unwrap(),
      6,
    );
    assert.equal(
      err<number, string>('x')
        .map((n) => n * 3)
        .unwrapOr(0),
      0,
    );
  });

  it('mapErr transforms err, leaves ok', () => {
    const e = err<number, string>('boom').mapErr((m) => m.toUpperCase());
    assert.equal(e.unwrapOr(0), 0);
    assert.deepEqual((e as Err<number, string>).error, 'BOOM');
  });

  it('andThen chains ok', () => {
    const r = ok<number, string>(2).andThen((n) => ok<number, string>(n + 1));
    assert.equal(r.unwrap(), 3);
  });

  it('andThen short-circuits on err', () => {
    let called = false;
    const r = err<number, string>('boom').andThen((n) => {
      called = true;
      return ok<number, string>(n + 1);
    });
    assert.equal(r.isErr(), true);
    assert.equal(called, false);
  });

  it('orElse recovers err', () => {
    const r = err<number, string>('boom').orElse(() => ok<number, string>(99));
    assert.equal(r.unwrap(), 99);
  });

  it('match invokes correct branch', () => {
    assert.equal(
      ok<number, string>(1).match(
        (v) => `ok:${v}`,
        () => 'err',
      ),
      'ok:1',
    );
    assert.equal(
      err<number, string>('x').match(
        () => 'ok',
        (e) => `err:${e}`,
      ),
      'err:x',
    );
  });

  it('combine resolves all ok', () => {
    const r = combine<number, string>([ok(1), ok(2), ok(3)]);
    assert.deepEqual(r.unwrap(), [1, 2, 3]);
  });

  it('combine short-circuits on first err', () => {
    const r = combine<number, string>([ok(1), err('boom'), ok(3)]);
    assert.equal(r.isErr(), true);
    assert.equal((r as Err<number[], string>).error, 'boom');
  });

  it('expect throws with custom message', () => {
    assert.throws(() => err<number, string>('boom').expect('expected ok'), /expected ok/);
  });

  it('Ok.unwrapErr throws', () => {
    assert.throws(() => ok<number, string>(1).unwrapErr(), /Ok\(1\)/);
  });

  it('Err.unwrapErr returns error', () => {
    assert.equal(err<number, string>('boom').unwrapErr(), 'boom');
  });
});
