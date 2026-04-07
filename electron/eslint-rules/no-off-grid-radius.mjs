/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Ban borderRadius values not in design system tokens.' },
    messages: {
      offGrid:
        'borderRadius {{value}} is not a design token. Use 4 (sm), 6 (md), 8 (lg), 10 (xl), or "50%".',
    },
  },
  create(context) {
    const ALLOWED_NUM = new Set([4, 6, 8, 10]);

    return {
      'JSXAttribute[name.name="style"] Property[key.name="borderRadius"]'(node) {
        const val = node.value;
        if (val.type === 'Literal') {
          if (typeof val.value === 'number' && !ALLOWED_NUM.has(val.value)) {
            context.report({ node: val, messageId: 'offGrid', data: { value: String(val.value) } });
          }
          if (typeof val.value === 'string' && val.value !== '50%' && !val.value.startsWith('var(')) {
            context.report({ node: val, messageId: 'offGrid', data: { value: val.value } });
          }
        }
      },
    };
  },
};
