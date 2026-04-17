import { isAppFile } from '../../../shared/constants.mjs';
import { RENDERER_DOMAINS } from '../../../shared/constants.mjs';

const DOMAIN_RE = new RegExp(`/domains/(${RENDERER_DOMAINS.join('|')})/(types|config|repo|service|runtime)/`);

const BANNED_IMPORT_PATTERNS = [
  { pattern: /\/providers\/http/, messageId: 'noHttpProvider' },
  { pattern: /^@tanstack\/react-query$/, messageId: 'noReactQuery' },
];

// Infrastructure specifiers that app/ legitimately needs (providers, query client setup).
const ALLOWED_SPECIFIERS = new Set([
  'QueryClientProvider', // Root React provider for app shell
  'useQueryClient', // Cache orchestration in the app-tier orchestration layer
  'OBS_DEGRADED_EVENT', // Event constant used in DataProvider bootstrap
]);

// Infrastructure provider paths that app/ legitimately needs.
const ALLOWED_PROVIDER_PATHS = new Set([
  '#renderer/global/providers/queryClient', // queryClient singleton for QueryClientProvider
]);

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'App files must be a thin orchestration layer: no domain internals, no HTTP providers, no direct react-query, no IPC.' },
    messages: {
      impure: 'App files must not import domain internals directly. Use the domain barrel or import UI by path. See s-arch skill.',
      noHttpProvider: 'App files must not import HTTP providers directly. Use repo mutation hooks. See s-arch skill.',
      noReactQuery: 'App files must not import from @tanstack/react-query directly. Use hooks from runtime/. See s-arch skill.',
      noIpc: 'App files must not access window.mandoAPI directly. Use runtime hooks. See s-arch skill.',
    },
  },
  create(context) {
    if (!isAppFile(context.filename || context.getFilename())) return {};

    return {
      ImportDeclaration(node) {
        const source = node.source.value;

        // Allow explicitly whitelisted provider paths (queryClient singleton, etc.)
        if (ALLOWED_PROVIDER_PATHS.has(source)) return;

        if (DOMAIN_RE.test(source)) {
          context.report({ node, messageId: 'impure' });
          return;
        }
        for (const { pattern, messageId } of BANNED_IMPORT_PATTERNS) {
          if (pattern.test(source)) {
            // Allow if ALL imported specifiers are in the infrastructure allowlist
            const specifiers = node.specifiers.filter((s) => s.type === 'ImportSpecifier');
            if (specifiers.length > 0 && specifiers.every((s) => ALLOWED_SPECIFIERS.has(s.imported.name))) {
              return;
            }
            context.report({ node, messageId });
            return;
          }
        }
      },
      MemberExpression(node) {
        if (
          node.object.type === 'Identifier' &&
          node.object.name === 'window' &&
          node.property.type === 'Identifier' &&
          node.property.name === 'mandoAPI'
        ) {
          context.report({ node, messageId: 'noIpc' });
        }
      },
    };
  },
};
