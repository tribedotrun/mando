// Bans `throw <string>` and `throw \`...\`` — typed Error instances are required.
// Pairs with @typescript-eslint/only-throw-error but catches template literals too.

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Throw an Error subclass; never throw a bare string.' },
    messages: {
      throwString: 'throw <string> is banned. Throw new Error(...) or a typed subclass.',
    },
    schema: [],
  },
  create(context) {
    return {
      ThrowStatement(node) {
        const arg = node.argument;
        if (!arg) return;
        if (arg.type === 'Literal' && typeof arg.value === 'string') {
          context.report({ node, messageId: 'throwString' });
        } else if (arg.type === 'TemplateLiteral') {
          context.report({ node, messageId: 'throwString' });
        }
      },
    };
  },
};
