/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Ban fontSize values not in the design system scale.' },
    messages: {
      offScale:
        'fontSize {{value}} is not in the design scale [11, 12, 13, 14, 16, 22, 32]. Use a .text-* class or a valid size.',
    },
  },
  create(context) {
    const ALLOWED = new Set([11, 12, 13, 14, 16, 22, 32]);

    return {
      'JSXAttribute[name.name="style"] Property[key.name="fontSize"]'(node) {
        const val = node.value;
        if (val.type === 'Literal' && typeof val.value === 'number' && !ALLOWED.has(val.value)) {
          context.report({ node: val, messageId: 'offScale', data: { value: String(val.value) } });
        }
      },
    };
  },
};
