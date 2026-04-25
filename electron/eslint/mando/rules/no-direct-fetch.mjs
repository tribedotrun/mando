// Bans direct `fetch(...)` calls outside the two designated funnel files.
// All HTTP must funnel through apiRequestInternal (renderer) or daemonRouteJson (main),
// which schema-validate every response.

const FUNNEL_FILES = [
  'src/renderer/global/providers/http.ts',
  'src/renderer/global/providers/httpRoutes.ts',
  'src/renderer/global/providers/httpObsQueue.ts',
  'src/main/global/runtime/lifecycle.ts',
  'src/main/global/runtime/daemonTransport.ts',
  // Updater fetches an external CF Worker feed and uses Node `https`, not `fetch`,
  // so it doesn't need an exception. The setup-validation Telegram fetch and
  // updater feed live outside the funnel by design.
  'src/main/onboarding/runtime/setupValidation.ts',
  'src/main/updater/runtime/updater.ts',
];

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Network fetch must funnel through schema-aware HTTP helpers.' },
    messages: {
      direct: 'Direct fetch() is banned outside funnel files. Use apiGetRoute / daemonRouteJson.',
    },
    schema: [],
  },
  create(context) {
    const filename = context.filename ?? context.getFilename();
    const isFunnel = FUNNEL_FILES.some((p) => filename.endsWith(p));
    if (isFunnel) return {};

    return {
      CallExpression(node) {
        if (node.callee.type === 'Identifier' && node.callee.name === 'fetch') {
          context.report({ node, messageId: 'direct' });
          return;
        }
        // Catch window.fetch(...), globalThis.fetch(...), self.fetch(...)
        if (
          node.callee.type === 'MemberExpression' &&
          !node.callee.computed &&
          node.callee.property.type === 'Identifier' &&
          node.callee.property.name === 'fetch' &&
          node.callee.object.type === 'Identifier' &&
          ['window', 'globalThis', 'self'].includes(node.callee.object.name)
        ) {
          context.report({ node, messageId: 'direct' });
        }
      },
    };
  },
};
