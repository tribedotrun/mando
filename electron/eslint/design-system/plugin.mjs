// Design tokens are `warn` (visible during Tailwind v4 migration, non-blocking).
import { RENDERER_TS } from '../shared/constants.mjs';
import noHardcodedColors from './rules/no-hardcoded-colors.mjs';
import noOffScaleFontSize from './rules/no-off-scale-font-size.mjs';
import noOffGridRadius from './rules/no-off-grid-radius.mjs';

const plugin = {
  rules: {
    'no-hardcoded-colors': noHardcodedColors,
    'no-off-scale-font-size': noOffScaleFontSize,
    'no-off-grid-radius': noOffGridRadius,
  },
};

export default [
  { plugins: { 'design-system': plugin } },
  {
    files: RENDERER_TS,
    rules: {
      'design-system/no-hardcoded-colors': 'warn',
      'design-system/no-off-scale-font-size': 'warn',
      'design-system/no-off-grid-radius': 'warn',
    },
  },
];
