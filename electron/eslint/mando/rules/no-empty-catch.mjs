// Empty .catch() bodies silently swallow errors.
// (Renamed from no-fire-and-forget — that name overpromised; this rule
// only checks .catch(). For true fire-and-forget detection see
// no-floating-void-call and the type-checked no-floating-promises.)

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Ban empty .catch() handlers.' },
    messages: {
      emptyCatch: 'Empty .catch() swallows errors silently. Log or handle the rejection.',
    },
  },
  create(context) {
    return {
      'CallExpression[callee.property.name="catch"]'(node) {
        if (node.arguments.length !== 1) return;
        const handler = node.arguments[0];
        if (handler.type !== 'ArrowFunctionExpression' && handler.type !== 'FunctionExpression') {
          return;
        }
        const body = handler.body;
        const isEmptyBlock = body.type === 'BlockStatement' && body.body.length === 0;
        const isUndefined = body.type === 'Identifier' && body.name === 'undefined';
        if (isEmptyBlock || isUndefined) {
          context.report({ node, messageId: 'emptyCatch' });
        }
      },
    };
  },
};
