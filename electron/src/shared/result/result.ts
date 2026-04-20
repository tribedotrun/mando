// Synchronous Result<T, E>. Mirrors neverthrow vocabulary so future migration is mechanical.
// All methods return new Results; instances are immutable.

export type Result<T, E> = Ok<T, E> | Err<T, E>;

export class Ok<T, E> {
  readonly _tag = 'ok' as const;
  readonly value: T;
  constructor(value: T) {
    this.value = value;
  }
  isOk(): this is Ok<T, E> {
    return true;
  }
  isErr(): this is Err<T, E> {
    return false;
  }
  map<U>(fn: (value: T) => U): Result<U, E> {
    return new Ok(fn(this.value));
  }
  mapErr<F>(_fn: (err: E) => F): Result<T, F> {
    return new Ok(this.value);
  }
  andThen<U, F>(fn: (value: T) => Result<U, F>): Result<U, E | F> {
    return fn(this.value);
  }
  orElse<U, F>(_fn: (err: E) => Result<U, F>): Result<T | U, F> {
    return new Ok(this.value);
  }
  match<R>(onOk: (value: T) => R, _onErr: (err: E) => R): R {
    return onOk(this.value);
  }
  unwrap(): T {
    return this.value;
  }
  unwrapOr<U>(_fallback: U): T | U {
    return this.value;
  }
  expect(_message: string): T {
    return this.value;
  }
  unwrapErr(): never {
    throw new Error(`called unwrapErr on Ok(${JSON.stringify(this.value)})`);
  }
}

export class Err<T, E> {
  readonly _tag = 'err' as const;
  readonly error: E;
  constructor(error: E) {
    this.error = error;
  }
  isOk(): this is Ok<T, E> {
    return false;
  }
  isErr(): this is Err<T, E> {
    return true;
  }
  map<U>(_fn: (value: T) => U): Result<U, E> {
    return new Err(this.error);
  }
  mapErr<F>(fn: (err: E) => F): Result<T, F> {
    return new Err(fn(this.error));
  }
  andThen<U, F>(_fn: (value: T) => Result<U, F>): Result<U, E | F> {
    return new Err(this.error);
  }
  orElse<U, F>(fn: (err: E) => Result<U, F>): Result<T | U, F> {
    return fn(this.error);
  }
  match<R>(_onOk: (value: T) => R, onErr: (err: E) => R): R {
    return onErr(this.error);
  }
  unwrap(): never {
    throw new Error(`called unwrap on Err(${JSON.stringify(this.error)})`);
  }
  unwrapOr<U>(fallback: U): T | U {
    return fallback;
  }
  expect(message: string): never {
    throw new Error(`${message}: ${JSON.stringify(this.error)}`);
  }
  unwrapErr(): E {
    return this.error;
  }
}

export function ok<T, E = never>(value: T): Result<T, E> {
  return new Ok(value);
}

export function err<T = never, E = never>(error: E): Result<T, E> {
  return new Err(error);
}

// Combine an array of Results: Ok if all Ok, first Err otherwise. Short-circuits.
export function combine<T, E>(results: Array<Result<T, E>>): Result<T[], E> {
  const out: T[] = [];
  for (const r of results) {
    if (r.isErr()) return new Err(r.error);
    out.push(r.value);
  }
  return new Ok(out);
}

export function isOk<T, E>(r: Result<T, E>): r is Ok<T, E> {
  return r.isOk();
}

export function isErr<T, E>(r: Result<T, E>): r is Err<T, E> {
  return r.isErr();
}
