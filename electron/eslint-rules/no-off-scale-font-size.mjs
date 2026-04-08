const DEFAULT_SCALE = [11, 12, 13, 14, 16, 22, 32];

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Ban fontSize values not in the design system scale.' },
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
      offScale:
        'fontSize {{value}} is not in the design scale [{{scale}}]. Use a .text-* class or a valid size.',
    },
  },
  create(context) {
    const opts = context.options[0] || {};
    const ALLOWED = new Set(opts.allowed || DEFAULT_SCALE);
    const scaleStr = [...ALLOWED].sort((a, b) => a - b).join(', ');

    return {
      'JSXAttribute[name.name="style"] Property[key.name="fontSize"]'(node) {
        const val = node.value;
        if (val.type === 'Literal' && typeof val.value === 'number' && !ALLOWED.has(val.value)) {
          context.report({ node: val, messageId: 'offScale', data: { value: String(val.value), scale: scaleStr } });
        }
      },
    };
  },
};
