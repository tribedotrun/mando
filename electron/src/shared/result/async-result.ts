// Async Result. Wraps Promise<Result<T, E>> with chainable combinators.
// Mirrors neverthrow ResultAsync; thenable so consumers can `await` it transparently.

import { Err, Ok, type Result, combine as combineSync, err, ok } from './result.ts';

export class ResultAsync<T, E> implements PromiseLike<Result<T, E>> {
  private readonly inner: Promise<Result<T, E>>;
  constructor(inner: Promise<Result<T, E>>) {
    this.inner = inner;
  }

  then<R1 = Result<T, E>, R2 = never>(
    onFulfilled?: ((value: Result<T, E>) => R1 | PromiseLike<R1>) | null,
    onRejected?: ((reason: unknown) => R2 | PromiseLike<R2>) | null,
  ): PromiseLike<R1 | R2> {
    return this.inner.then(onFulfilled, onRejected);
  }

  map<U>(fn: (value: T) => U | Promise<U>): ResultAsync<U, E> {
    return new ResultAsync<U, E>(
      this.inner.then(async (r) => {
        if (r.isErr()) return new Err<U, E>(r.error);
        return new Ok<U, E>(await fn(r.value));
      }),
    );
  }

  mapErr<F>(fn: (err: E) => F | Promise<F>): ResultAsync<T, F> {
    return new ResultAsync<T, F>(
      this.inner.then(async (r) => {
        if (r.isOk()) return new Ok<T, F>(r.value);
        return new Err<T, F>(await fn(r.error));
      }),
    );
  }

  andThen<U, F>(
    fn: (value: T) => ResultAsync<U, F> | Promise<Result<U, F>> | Result<U, F>,
  ): ResultAsync<U, E | F> {
    return new ResultAsync<U, E | F>(
      this.inner.then(async (r) => {
        if (r.isErr()) return new Err<U, E | F>(r.error);
        const next = fn(r.value);
        if (next instanceof ResultAsync) return next.toPromise();
        return await next;
      }),
    );
  }

  orElse<U, F>(
    fn: (err: E) => ResultAsync<U, F> | Promise<Result<U, F>> | Result<U, F>,
  ): ResultAsync<T | U, F> {
    return new ResultAsync<T | U, F>(
      this.inner.then(async (r) => {
        if (r.isOk()) return new Ok<T | U, F>(r.value);
        const next = fn(r.error);
        if (next instanceof ResultAsync) return next.toPromise();
        return await next;
      }),
    );
  }

  async match<R>(onOk: (value: T) => R, onErr: (err: E) => R): Promise<R> {
    const r = await this.inner;
    return r.match(onOk, onErr);
  }

  toPromise(): Promise<Result<T, E>> {
    return this.inner;
  }

  // unwrapOr resolves to T | U, never throws.
  async unwrapOr<U>(fallback: U): Promise<T | U> {
    const r = await this.inner;
    return r.unwrapOr(fallback);
  }
}

export function okAsync<T, E = never>(value: T): ResultAsync<T, E> {
  return new ResultAsync<T, E>(Promise.resolve(ok<T, E>(value)));
}

export function errAsync<T = never, E = never>(error: E): ResultAsync<T, E> {
  return new ResultAsync<T, E>(Promise.resolve(err<T, E>(error)));
}

// Wrap a throwing Promise. errorFn maps the caught throw to E.
export function fromPromise<T, E>(
  promise: Promise<T>,
  errorFn: (cause: unknown) => E,
): ResultAsync<T, E> {
  return new ResultAsync<T, E>(
    promise.then(
      (value) => ok<T, E>(value),
      (cause: unknown) => err<T, E>(errorFn(cause)),
    ),
  );
}

// Wrap a known-safe Promise (already returns Result-like values via convention).
export function fromSafePromise<T, E = never>(promise: Promise<T>): ResultAsync<T, E> {
  return new ResultAsync<T, E>(promise.then((value) => ok<T, E>(value)));
}

// Wrap a throwing function. Returns Result, not ResultAsync.
export function fromThrowable<TArgs extends unknown[], T, E>(
  fn: (...args: TArgs) => T,
  errorFn: (cause: unknown) => E,
): (...args: TArgs) => Result<T, E> {
  return (...args: TArgs) => {
    try {
      return ok<T, E>(fn(...args));
    } catch (cause) {
      return err<T, E>(errorFn(cause));
    }
  };
}

// Combine an array of ResultAsyncs. Resolves to Ok([]) if all Ok, first Err otherwise.
export function combineAsync<T, E>(items: Array<ResultAsync<T, E>>): ResultAsync<T[], E> {
  return new ResultAsync<T[], E>(
    Promise.all(items.map((it) => it.toPromise())).then((results) => combineSync(results)),
  );
}

// Sequence: same as combineAsync but for clarity at call sites that semantically iterate.
export const sequence = combineAsync;
