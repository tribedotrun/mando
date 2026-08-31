// Project correctness rules for typed boundaries and observable async failures.
import { ALL_TS } from '../shared/constants.mjs';
import noAsOnBoundary from './rules/no-as-on-boundary.mjs';
import noDirectFetch from './rules/no-direct-fetch.mjs';
import noBareThrow from './rules/no-bare-throw.mjs';
import requireSchemaOnFunnel from './rules/require-schema-on-funnel.mjs';
import requireResultReturn from './rules/require-result-return.mjs';
import noUnusedErrorParam from './rules/no-unused-error-param.mjs';
import noPromiseChaining from './rules/no-promise-chaining.mjs';

// Note: actual fire-and-forget detection lives in
// @typescript-eslint/no-floating-promises (configured in external.mjs).
// `void promise()` is the documented escape hatch and is intentionally NOT
// banned — the React Query / SSE layers depend on it.

const plugin = {
  rules: {
    'no-as-on-boundary': noAsOnBoundary,
    'no-direct-fetch': noDirectFetch,
    'no-bare-throw': noBareThrow,
    'require-schema-on-funnel': requireSchemaOnFunnel,
    'require-result-return': requireResultReturn,
    'no-unused-error-param': noUnusedErrorParam,
    'no-promise-chaining': noPromiseChaining,
  },
};

export default [
  { plugins: { mando: plugin } },
  {
    files: ALL_TS,
    rules: {
      'mando/no-as-on-boundary': 'error',
      'mando/no-direct-fetch': 'error',
      'mando/no-bare-throw': 'error',
      'mando/require-schema-on-funnel': 'error',
      'mando/require-result-return': 'error',
      'mando/no-unused-error-param': 'error',
      'mando/no-promise-chaining': 'error',
      // PR #883 invariant #2: ban empty try/catch. Promise `.catch()` chains
      // are rejected separately by mando/no-promise-chaining.
      'no-empty': ['error', { allowEmptyCatch: false }],
      // PR #883 invariant #3: ban console.* in production code. The
      // preload IPC validator and renderer/main logger self-referential
      // failure paths are allowed via per-file overrides in
      // tests-overrides.mjs.
      'no-console': 'error',
    },
  },
  // PR #883 invariant #3: named exemptions from `no-console`. These two
  // files are the project's self-referential failure paths — the logger
  // cannot log its own rotation failures without infinite recursion, and
  // the IPC runtime must fail loudly on schema rejection before the
  // renderer logger is available. Keep the allowlist tiny and explicit.
  {
    files: [
      'src/main/global/providers/logger.ts',
      'src/shared/ipc-contract/runtime.ts',
    ],
    rules: { 'no-console': 'off' },
  },
];
