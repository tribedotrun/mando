import eslint from '@eslint/js';
import tseslint from 'typescript-eslint';

// Shared pattern fragments
const BAN_RELATIVE = {
  group: ['./*', '../*'],
  message: 'Use #renderer/ or #main/ aliases instead of relative imports.',
};
const BAN_MAIN = { group: ['#main/*'], message: 'renderer cannot import from main.' };
const BAN_RENDERER = { group: ['#renderer/*'], message: 'main cannot import from renderer.' };

export default tseslint.config(
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  {
    rules: {
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
      '@typescript-eslint/no-explicit-any': 'warn',
    },
  },

  // ── Ban useEffect — use useMountEffect, useQuery, derived state, or event handlers ──
  {
    files: ['src/**/*.ts', 'src/**/*.tsx'],
    ignores: ['src/renderer/hooks/useMountEffect.ts'],
    rules: {
      'no-restricted-syntax': [
        'error',
        {
          selector: 'CallExpression[callee.name="useEffect"]',
          message:
            'useEffect is banned. Use useMountEffect, useQuery, derived state, or event handlers.',
        },
        {
          selector: 'CallExpression[callee.name="useLayoutEffect"]',
          message:
            'useLayoutEffect is banned. Use useMountEffect, ref callbacks, or event handlers.',
        },
        {
          selector: 'CallExpression[callee.object.name="React"][callee.property.name="useEffect"]',
          message:
            'useEffect is banned. Use useMountEffect, useQuery, derived state, or event handlers.',
        },
        {
          selector:
            'CallExpression[callee.object.name="React"][callee.property.name="useLayoutEffect"]',
          message:
            'useLayoutEffect is banned. Use useMountEffect, ref callbacks, or event handlers.',
        },
      ],
    },
  },

  // ── Generic: renderer files — ban relative imports + ban #main ──
  // (overridden by more specific layer rules below for types/api/stores/hooks)
  {
    files: ['src/renderer/**/*.ts', 'src/renderer/**/*.tsx'],
    rules: {
      'no-restricted-imports': ['error', { patterns: [BAN_RELATIVE, BAN_MAIN] }],
    },
  },

  // ── Main process — ban relative + ban renderer ──
  {
    files: ['src/main/**/*.ts'],
    rules: {
      'no-restricted-imports': ['error', { patterns: [BAN_RELATIVE, BAN_RENDERER] }],
    },
  },

  // ── Preload — ban relative + ban both main and renderer ──
  {
    files: ['src/preload/**/*.ts'],
    rules: {
      'no-restricted-imports': ['error', { patterns: [BAN_RELATIVE, BAN_MAIN, BAN_RENDERER] }],
    },
  },

  // ── Layer: types — no imports from higher layers ──
  // (placed AFTER generic renderer block so it wins in flat config)
  {
    files: ['src/renderer/types.ts'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            BAN_RELATIVE,
            BAN_MAIN,
            {
              group: [
                '#renderer/api',
                '#renderer/stores/*',
                '#renderer/hooks/*',
                '#renderer/components/*',
              ],
              message: 'types cannot import from higher layers.',
            },
          ],
        },
      ],
    },
  },

  // ── Layer: api — can only import types (+ sibling api-* modules) ──
  {
    files: ['src/renderer/api.ts', 'src/renderer/api-*.ts'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            BAN_RELATIVE,
            BAN_MAIN,
            {
              group: ['#renderer/stores/*', '#renderer/hooks/*', '#renderer/components/*'],
              message: 'api cannot import from stores/hooks/components.',
            },
          ],
        },
      ],
    },
  },

  // ── Layer: stores — can import types + api only ──
  {
    files: ['src/renderer/stores/**/*.ts'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            BAN_RELATIVE,
            BAN_MAIN,
            {
              group: ['#renderer/hooks/*', '#renderer/components/*'],
              message: 'stores cannot import from hooks/components.',
            },
          ],
        },
      ],
    },
  },

  // ── Layer: hooks — can import types + api + stores only ──
  {
    files: ['src/renderer/hooks/**/*.ts'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            BAN_RELATIVE,
            BAN_MAIN,
            {
              group: ['#renderer/components/*'],
              message: 'hooks cannot import from components.',
            },
          ],
        },
      ],
    },
  },

  // ── Allow CSS/asset relative imports in entry point (Vite handles these) ──
  {
    files: ['src/renderer/index.tsx'],
    rules: {
      'no-restricted-imports': 'off',
    },
  },

  {
    ignores: ['dist/', '.vite/', '.test-build/', 'node_modules/'],
  },
);
