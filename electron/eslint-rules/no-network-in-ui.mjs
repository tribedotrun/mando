/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Ban fetch() and direct network calls in component files.' },
    messages: {
      noFetch: 'Components must not call fetch() directly. Use the API layer via hooks or stores.',
    },
  },
  create(context) {
    const filename = context.filename || context.getFilename();
    const isComponent = /\/components\//.test(filename);
    if (!isComponent) return {};

    return {
      'CallExpression[callee.name="fetch"]'(node) {
        context.report({ node, messageId: 'noFetch' });
      },
      'CallExpression[callee.property.name="fetch"]'(node) {
        context.report({ node, messageId: 'noFetch' });
      },
    };
  },
};
