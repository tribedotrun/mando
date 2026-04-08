/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Ban duplicate import statements from the same module.' },
    messages: {
      duplicate:
        "'{{source}}' imported multiple times. Merge into a single import statement.",
    },
  },
  create(context) {
    const seen = new Map();

    return {
      ImportDeclaration(node) {
        const source = node.source.value;
        const prev = seen.get(source);
        if (prev) {
          context.report({ node, messageId: 'duplicate', data: { source } });
        } else {
          seen.set(source, node);
        }
      },
    };
  },
};
