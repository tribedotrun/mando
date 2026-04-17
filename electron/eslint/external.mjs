// Vendor presets plus the minimum set of rules recommended does not cover.

import { resolve } from 'node:path';
import eslint from '@eslint/js';
import tseslint from 'typescript-eslint';
import { ALL_TS } from './shared/constants.mjs';

const ELECTRON_DIR = resolve(import.meta.dirname, '..');

export default [
  eslint.configs.recommended,
  ...tseslint.configs.recommended,

  // Type-aware rules: require the TS language service (enabled via projectService). Narrow pick of the three that catch real correctness bugs; the full recommendedTypeChecked preset would be noise.
  {
    files: ALL_TS,
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: ELECTRON_DIR,
      },
    },
    rules: {
      '@typescript-eslint/no-floating-promises': 'error',
      '@typescript-eslint/no-misused-promises': 'error',
      '@typescript-eslint/await-thenable': 'error',
    },
  },

  {
    rules: {
      // Escape hatch for intentionally unused params: prefix with _.
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
      // Not in recommended. Duplicate imports from the same module are legal JS but messy; force consolidation.
      'no-duplicate-imports': 'error',
    },
  },
];
