// ESLint composition entry. Order: external base -> custom plugins -> architecture (purity + imports) -> ignores.

import external from './external.mjs';

import mando from './mando/plugin.mjs';

import architecture from './architecture/index.mjs';

export default [
  ...external,

  ...mando,

  ...architecture,

  { ignores: ['dist/', '.vite/', '.test-build/', 'node_modules/'] },
];
