// ESLint composition entry. Order: external base -> custom plugins -> architecture (purity + imports) -> tests overrides -> ignores.

import external from './external.mjs';

import designSystem from './design-system/plugin.mjs';
import mando from './mando/plugin.mjs';
import syntaxBans from './mando/syntax-bans.mjs';

import architecture from './architecture/index.mjs';

import testsOverrides from './tests-overrides.mjs';

export default [
  ...external,

  ...designSystem,
  ...mando,
  ...syntaxBans,

  ...architecture,

  ...testsOverrides,

  { ignores: ['dist/', '.vite/', '.test-build/', 'node_modules/'] },
];
