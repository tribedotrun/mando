/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Ban empty .catch() handlers that silently swallow errors.' },
    messages: {
      emptyCatch:
        'Empty .catch() swallows errors silently. Log or handle the rejection.',
    },
  },
  create(context) {
    return {
      // .catch(() => {}) or .catch(() => undefined)
      'CallExpression[callee.property.name="catch"]'(node) {
        if (node.arguments.length !== 1) return;
        const handler = node.arguments[0];
        if (handler.type === 'ArrowFunctionExpression' || handler.type === 'FunctionExpression') {
          const body = handler.body;
          if (body.type === 'BlockStatement' && body.body.length === 0) {
            context.report({ node, messageId: 'emptyCatch' });
          }
          if (body.type === 'Identifier' && body.name === 'undefined') {
            context.report({ node, messageId: 'emptyCatch' });
          }
        }
      },
    };
  },
};
