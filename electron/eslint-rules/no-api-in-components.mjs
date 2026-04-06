/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Ban direct API imports in component files. Use hooks or stores.' },
    messages: {
      noApi:
        'Components must not import from the API layer directly. Move this call into a hook or store action.',
    },
  },
  create(context) {
    const filename = context.filename || context.getFilename();
    const isComponent = /\/components\//.test(filename);
    if (!isComponent) return {};

    return {
      ImportDeclaration(node) {
        const src = node.source.value;
        if (src === '#renderer/api' || src === '#renderer/api-scout' || /^#renderer\/api-/.test(src)) {
          context.report({ node, messageId: 'noApi' });
        }
      },
    };
  },
};
