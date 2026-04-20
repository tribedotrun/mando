// Public surface of the in-house Result library. Only file consumers may import from outside.
// Imports of #result/internal/* or relative paths into this folder from outside are linted.

export {
  type ApiError,
  ApiErrorThrown,
  apiErrorMessage,
  httpError,
  parseError,
  networkError,
  timeoutError,
  ipcError,
  ioError,
  invariantError,
} from './errors.ts';

export { type Result, Ok, Err, ok, err, combine, isOk, isErr } from './result.ts';

export {
  ResultAsync,
  okAsync,
  errAsync,
  fromPromise,
  fromSafePromise,
  fromThrowable,
  combineAsync,
  sequence,
} from './async-result.ts';

export {
  parseWith,
  parseJsonText,
  parseJsonTextWith,
  fromResponse,
  fromSseMessage,
  fromIpc,
  fromFile,
  toReactQuery,
} from './helpers.ts';

export { type SseParseResult, type SseParseFailure, parseSseMessage } from './sse-parse-handler.ts';

export { SchemaParseError } from './schema-parse-error.ts';
