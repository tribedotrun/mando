// Project hygiene rules. All errors — these encode invariants the
// codebase relies on (no swallowed errors, no fire-and-forget, no DOM clashes).
import { ALL_TS, RENDERER_TS } from '../shared/constants.mjs';
import noEmptyCatch from './rules/no-empty-catch.mjs';
import noMagicTimeouts from './rules/no-magic-timeouts.mjs';
import noDirectDomMutation from './rules/no-direct-dom-mutation.mjs';
import noSelfImport from './rules/no-self-import.mjs';
import noAsOnBoundary from './rules/no-as-on-boundary.mjs';
import noDirectFetch from './rules/no-direct-fetch.mjs';
import noDirectThirdPartyErrorLibs from './rules/no-direct-third-party-error-libs.mjs';
import noThrowString from './rules/no-throw-string.mjs';
import noDeepResultImport from './rules/no-deep-result-import.mjs';
import requireEslintDisableReason from './rules/require-eslint-disable-reason.mjs';
import noBareThrow from './rules/no-bare-throw.mjs';
import requireSchemaOnFunnel from './rules/require-schema-on-funnel.mjs';
import requireResultReturn from './rules/require-result-return.mjs';
import noUnusedErrorParam from './rules/no-unused-error-param.mjs';

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
    'no-as-on-boundary': noAsOnBoundary,
    'no-direct-fetch': noDirectFetch,
    'no-direct-third-party-error-libs': noDirectThirdPartyErrorLibs,
    'no-throw-string': noThrowString,
    'no-deep-result-import': noDeepResultImport,
    'require-eslint-disable-reason': requireEslintDisableReason,
    'no-bare-throw': noBareThrow,
    'require-schema-on-funnel': requireSchemaOnFunnel,
    'require-result-return': requireResultReturn,
    'no-unused-error-param': noUnusedErrorParam,
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
      'mando/no-as-on-boundary': 'error',
      'mando/no-direct-fetch': 'error',
      'mando/no-direct-third-party-error-libs': 'error',
      'mando/no-throw-string': 'error',
      'mando/no-deep-result-import': 'error',
      'mando/require-eslint-disable-reason': 'error',
      'mando/no-bare-throw': 'error',
      'mando/require-schema-on-funnel': 'error',
      'mando/require-result-return': 'error',
      'mando/no-unused-error-param': 'error',
      // PR #883 invariant #2: ban empty try/catch (ESLint core no-empty
      // already runs at 'error' by default, we just disable the
      // allowEmptyCatch carve-out). The mando/no-empty-catch rule
      // complements this by covering `.catch(() => {})`.
      'no-empty': ['error', { allowEmptyCatch: false }],
      // PR #883 invariant #3: ban console.* in production code. The
      // preload IPC validator and renderer/main logger self-referential
      // failure paths are allowed via per-file overrides in
      // tests-overrides.mjs.
      'no-console': 'error',
    },
  },
  {
    files: RENDERER_TS,
    rules: { 'mando/no-direct-dom-mutation': 'error' },
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
