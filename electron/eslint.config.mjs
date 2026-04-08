import eslint from '@eslint/js';
import tseslint from 'typescript-eslint';
import noHardcodedColors from './eslint-rules/no-hardcoded-colors.mjs';
import noOffScaleFontSize from './eslint-rules/no-off-scale-font-size.mjs';
import noOffGridRadius from './eslint-rules/no-off-grid-radius.mjs';
import noApiInComponents from './eslint-rules/no-api-in-components.mjs';
import noBusinessLogicInUi from './eslint-rules/no-business-logic-in-ui.mjs';
import noNetworkInUi from './eslint-rules/no-network-in-ui.mjs';
import noFireAndForget from './eslint-rules/no-fire-and-forget.mjs';
import noMagicTimeouts from './eslint-rules/no-magic-timeouts.mjs';
import noDirectDomMutation from './eslint-rules/no-direct-dom-mutation.mjs';
import noDuplicateImports from './eslint-rules/no-duplicate-imports.mjs';
import noSelfImport from './eslint-rules/no-self-import.mjs';

// ── Shared patterns ──

const BAN_RELATIVE = {
  group: ['./*', '../*'],
  message: 'Use #renderer/ or #main/ aliases instead of relative imports.',
};
// NOTE: ESLint's no-restricted-imports uses the `ignore` npm package for pattern
// matching. In that library, `#` is the comment character. Prefix with `\` to
// match the literal `#` in TypeScript path aliases like `\\#renderer/...`.
const BAN_MAIN = { group: ['\\#main/*'], message: 'renderer cannot import from main.' };
const BAN_RENDERER = {
  group: ['\\#renderer/*'],
  message: 'main cannot import from renderer.',
};

// Domain names for import isolation
const DOMAINS = ['captain', 'scout', 'sessions', 'settings', 'onboarding', 'terminal'];

// Build patterns that ban importing another domain's internals (stores/hooks/components)
// but allow barrel imports (#renderer/domains/{domain} or #renderer/domains/{domain}/index)
function banOtherDomainInternals(ownDomain) {
  return DOMAINS.filter((d) => d !== ownDomain).flatMap((d) => [
    {
      group: [`\\#renderer/domains/${d}/**`],
      message: `Domain "${ownDomain}" cannot import "${d}" internals. Use the barrel (#renderer/domains/${d}).`,
    },
  ]);
}

export default tseslint.config(
  eslint.configs.recommended,
  ...tseslint.configs.recommended,

  // ══════════════════════════════════════════════════════════════
  //  TYPESCRIPT-ESLINT TYPE-CHECKED (cherry-picked)
  // ══════════════════════════════════════════════════════════════
  {
    files: ['src/**/*.ts', 'src/**/*.tsx'],
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      '@typescript-eslint/no-floating-promises': 'error',
      '@typescript-eslint/no-misused-promises': 'error',
      '@typescript-eslint/await-thenable': 'error',
    },
  },

  // ══════════════════════════════════════════════════════════════
  //  CUSTOM PLUGINS
  // ══════════════════════════════════════════════════════════════

  // ── Register custom plugins ──
  {
    plugins: {
      'design-system': {
        rules: {
          'no-hardcoded-colors': noHardcodedColors,
          'no-off-scale-font-size': noOffScaleFontSize,
          'no-off-grid-radius': noOffGridRadius,
        },
      },
      arch: {
        rules: {
          'no-api-in-components': noApiInComponents,
          'no-business-logic-in-ui': noBusinessLogicInUi,
          'no-network-in-ui': noNetworkInUi,
        },
      },
      mando: {
        rules: {
          'no-fire-and-forget': noFireAndForget,
          'no-magic-timeouts': noMagicTimeouts,
          'no-direct-dom-mutation': noDirectDomMutation,
          'no-duplicate-imports': noDuplicateImports,
          'no-self-import': noSelfImport,
        },
      },
    },
  },

  // ── Global rules ──
  {
    rules: {
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
      '@typescript-eslint/no-explicit-any': 'warn',
    },
  },

  // ── Ban useEffect + hover handlers ──
  {
    files: ['src/**/*.ts', 'src/**/*.tsx'],
    ignores: [
      'src/renderer/global/hooks/useMountEffect.ts',
      'src/renderer/global/hooks/useDraft.ts',
    ],
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
        {
          selector: 'JSXAttribute[name.name="onMouseEnter"]',
          message: 'Inline hover handlers are banned. Use CSS :hover or Tailwind hover: classes.',
        },
        {
          selector: 'JSXAttribute[name.name="onMouseLeave"]',
          message: 'Inline hover handlers are banned. Use CSS :hover or Tailwind hover: classes.',
        },
      ],
    },
  },

  // ── Design system rules (all renderer files) ──
  {
    files: ['src/renderer/**/*.ts', 'src/renderer/**/*.tsx'],
    rules: {
      'design-system/no-hardcoded-colors': 'error',
      'design-system/no-off-scale-font-size': 'error',
      'design-system/no-off-grid-radius': 'error',
    },
  },

  // ── Architecture purity rules (component files only) ──
  {
    files: ['src/renderer/**/components/**/*.tsx'],
    rules: {
      'arch/no-api-in-components': 'error',
      'arch/no-business-logic-in-ui': 'error',
      'arch/no-network-in-ui': 'error',
    },
  },

  // ── Mando custom rules (all source files) ──
  {
    files: ['src/**/*.ts', 'src/**/*.tsx'],
    rules: {
      'mando/no-fire-and-forget': 'error',
      'mando/no-magic-timeouts': 'error',
      'mando/no-duplicate-imports': 'error',
      'mando/no-self-import': 'error',
    },
  },

  // ── DOM mutation ban (renderer only) ──
  {
    files: ['src/renderer/**/*.ts', 'src/renderer/**/*.tsx'],
    rules: {
      'mando/no-direct-dom-mutation': 'error',
    },
  },

  // ══════════════════════════════════════════════════════════════
  //  PROCESS ISOLATION: main / renderer / preload
  // ══════════════════════════════════════════════════════════════

  // ── Generic renderer — ban relative + ban main ──
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

  // ── Preload — ban relative + ban both ──
  {
    files: ['src/preload/**/*.ts'],
    rules: {
      'no-restricted-imports': ['error', { patterns: [BAN_RELATIVE, BAN_MAIN, BAN_RENDERER] }],
    },
  },

  // ══════════════════════════════════════════════════════════════
  //  FOUNDATION LAYER: types, api, utils, styles, logger, queryClient
  // ══════════════════════════════════════════════════════════════

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
              group: ['\\#renderer/domains/**', '\\#renderer/global/**', '\\#renderer/app/**'],
              message:
                'types.ts is a foundation file — cannot import from domains, global, or app.',
            },
          ],
        },
      ],
    },
  },

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
              group: ['\\#renderer/domains/**', '\\#renderer/global/**', '\\#renderer/app/**'],
              message: 'API files are foundation — cannot import from domains, global, or app.',
            },
          ],
        },
      ],
    },
  },

  // ══════════════════════════════════════════════════════════════
  //  GLOBAL LAYER: shared components, stores, hooks, utils
  //  Can import: foundation. Cannot import: domains, app.
  // ══════════════════════════════════════════════════════════════

  {
    files: ['src/renderer/global/**/*.ts', 'src/renderer/global/**/*.tsx'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            BAN_RELATIVE,
            BAN_MAIN,
            {
              group: ['\\#renderer/domains/**', '\\#renderer/app/**'],
              message: 'Global layer cannot import from domains or app.',
            },
          ],
        },
      ],
    },
  },

  // Global layer chain: stores can't import hooks/components
  {
    files: ['src/renderer/global/stores/**/*.ts'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            BAN_RELATIVE,
            BAN_MAIN,
            {
              group: [
                '\\#renderer/domains/**',
                '\\#renderer/app/**',
                '\\#renderer/global/hooks/**',
                '\\#renderer/global/components/**',
              ],
              message: 'Global stores cannot import from hooks, components, domains, or app.',
            },
          ],
        },
      ],
    },
  },

  // Global hooks can't import components
  {
    files: ['src/renderer/global/hooks/**/*.ts'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            BAN_RELATIVE,
            BAN_MAIN,
            {
              group: [
                '\\#renderer/domains/**',
                '\\#renderer/app/**',
                '\\#renderer/global/components/**',
              ],
              message: 'Global hooks cannot import from components, domains, or app.',
            },
          ],
        },
      ],
    },
  },

  // ══════════════════════════════════════════════════════════════
  //  DOMAIN LAYERS: each domain isolated from other domain internals
  //  Can import: own internals, other domain barrels, global, foundation
  // ══════════════════════════════════════════════════════════════

  ...DOMAINS.map((domain) => ({
    files: [`src/renderer/domains/${domain}/**/*.ts`, `src/renderer/domains/${domain}/**/*.tsx`],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            BAN_RELATIVE,
            BAN_MAIN,
            { group: ['\\#renderer/app/**'], message: 'Domains cannot import from app.' },
            ...banOtherDomainInternals(domain),
          ],
        },
      ],
    },
  })),

  // Domain layer chain: stores can't import hooks/components within each domain
  ...DOMAINS.map((domain) => ({
    files: [`src/renderer/domains/${domain}/stores/**/*.ts`],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            BAN_RELATIVE,
            BAN_MAIN,
            { group: ['\\#renderer/app/**'], message: 'Domains cannot import from app.' },
            ...banOtherDomainInternals(domain),
            {
              group: [
                `\\#renderer/domains/${domain}/hooks/**`,
                `\\#renderer/domains/${domain}/components/**`,
              ],
              message: 'Stores cannot import from hooks or components.',
            },
          ],
        },
      ],
    },
  })),

  // Domain hooks can't import components
  ...DOMAINS.map((domain) => ({
    files: [`src/renderer/domains/${domain}/hooks/**/*.ts`],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            BAN_RELATIVE,
            BAN_MAIN,
            { group: ['\\#renderer/app/**'], message: 'Domains cannot import from app.' },
            ...banOtherDomainInternals(domain),
            {
              group: [`\\#renderer/domains/${domain}/components/**`],
              message: 'Hooks cannot import from components.',
            },
          ],
        },
      ],
    },
  })),

  // ══════════════════════════════════════════════════════════════
  //  APP LAYER: assembly wiring — can import everything
  // ══════════════════════════════════════════════════════════════

  {
    files: ['src/renderer/app/**/*.ts', 'src/renderer/app/**/*.tsx'],
    rules: {
      'no-restricted-imports': ['error', { patterns: [BAN_RELATIVE, BAN_MAIN] }],
    },
  },

  // ── Allow CSS/asset relative imports in entry point ──
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
