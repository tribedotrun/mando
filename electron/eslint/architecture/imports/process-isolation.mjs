import { RENDERER_TS, MAIN_TS, PRELOAD_TS } from '../../shared/constants.mjs';
import { BAN_RELATIVE, BAN_MAIN, BAN_RENDERER, restrictImports } from '../../shared/helpers.mjs';

export default [
  {
    files: RENDERER_TS,
    rules: restrictImports(BAN_RELATIVE, BAN_MAIN),
  },
  {
    files: MAIN_TS,
    rules: restrictImports(BAN_RELATIVE, BAN_RENDERER),
  },
  {
    files: PRELOAD_TS,
    rules: restrictImports(BAN_RELATIVE, BAN_MAIN, BAN_RENDERER),
  },
  {
    files: ['src/renderer/index.tsx'],
    rules: { 'no-restricted-imports': 'off' },
  },
];
