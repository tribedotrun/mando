// Project hygiene rules. All errors — these encode invariants the
// codebase relies on (no swallowed errors, no fire-and-forget, no DOM clashes).
import { ALL_TS, RENDERER_TS } from '../shared/constants.mjs';
import noEmptyCatch from './rules/no-empty-catch.mjs';
import noMagicTimeouts from './rules/no-magic-timeouts.mjs';
import noDirectDomMutation from './rules/no-direct-dom-mutation.mjs';
import noSelfImport from './rules/no-self-import.mjs';

// Note: actual fire-and-forget detection lives in
// @typescript-eslint/no-floating-promises (configured in external.mjs).
// `void promise()` is the documented escape hatch and is intentionally NOT
// banned — the React Query / SSE layers depend on it.

const plugin = {
  rules: {
    'no-empty-catch': noEmptyCatch,
    'no-magic-timeouts': noMagicTimeouts,
    'no-direct-dom-mutation': noDirectDomMutation,
    'no-self-import': noSelfImport,
  },
};

export default [
  { plugins: { mando: plugin } },
  {
    files: ALL_TS,
    rules: {
      'mando/no-empty-catch': 'error',
      'mando/no-magic-timeouts': 'error',
      'mando/no-self-import': 'error',
    },
  },
  {
    files: RENDERER_TS,
    rules: { 'mando/no-direct-dom-mutation': 'error' },
  },
];
