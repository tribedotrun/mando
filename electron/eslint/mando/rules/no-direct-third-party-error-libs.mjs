// Bans third-party Result/error-handling libraries. The in-house #result module is
// the only allowed source. Belt-and-braces guard against future contributors reaching
// for a library when they should extend the in-house module.

const BANNED = ['neverthrow', 'oxide.ts', 'ts-results', 'ts-results-es', 'effect'];

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Ban imports of third-party Result/error libraries.' },
    messages: {
      banned:
        "Importing '{{name}}' is banned. Use the in-house Result module via `import ... from '#result'`.",
    },
    schema: [],
  },
  create(context) {
    function check(source, node) {
      if (BANNED.some((pkg) => source === pkg || source.startsWith(pkg + '/'))) {
        context.report({ node, messageId: 'banned', data: { name: source } });
      }
    }
    return {
      ImportDeclaration(node) {
        check(node.source.value, node);
      },
      ImportExpression(node) {
        if (node.source.type === 'Literal' && typeof node.source.value === 'string') {
          check(node.source.value, node);
        }
      },
    };
  },
};
