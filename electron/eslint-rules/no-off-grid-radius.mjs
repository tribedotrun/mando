const DEFAULT_RADII = [4, 6, 8, 10];

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Ban borderRadius values not in design system tokens.' },
    schema: [
      {
        type: 'object',
        properties: {
          allowed: { type: 'array', items: { type: 'number' } },
        },
        additionalProperties: false,
      },
    ],
    messages: {
      offGrid:
        'borderRadius {{value}} is not a design token. Allowed: {{tokens}} or "50%".',
    },
  },
  create(context) {
    const opts = context.options[0] || {};
    const ALLOWED_NUM = new Set(opts.allowed || DEFAULT_RADII);
    const tokensStr = [...ALLOWED_NUM].sort((a, b) => a - b).join(', ');

    return {
      'JSXAttribute[name.name="style"] Property[key.name="borderRadius"]'(node) {
        const val = node.value;
        if (val.type === 'Literal') {
          if (typeof val.value === 'number' && !ALLOWED_NUM.has(val.value)) {
            context.report({ node: val, messageId: 'offGrid', data: { value: String(val.value), tokens: tokensStr } });
          }
          if (typeof val.value === 'string' && val.value !== '50%' && !val.value.startsWith('var(')) {
            context.report({ node: val, messageId: 'offGrid', data: { value: val.value, tokens: tokensStr } });
          }
        }
      },
    };
  },
};
