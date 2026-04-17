// Bans fontSize values not in the design scale, in inline styles AND in
// Tailwind arbitrary classNames like `text-[15px]`.

const DEFAULT_SCALE = [11, 12, 13, 14, 16, 22, 32];
const ARBITRARY_RE = /(?:^|\s)text-\[(\d+(?:\.\d+)?)px\]/g;

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Ban fontSize values outside the design scale.' },
    schema: [
      {
        type: 'object',
        properties: { allowed: { type: 'array', items: { type: 'number' } } },
        additionalProperties: false,
      },
    ],
    messages: {
      offScale: 'fontSize {{value}} is not in the scale [{{scale}}]. Use a .text-* class or a valid size.',
    },
  },
  create(context) {
    const allowed = new Set(context.options[0]?.allowed || DEFAULT_SCALE);
    const scaleStr = [...allowed].sort((a, b) => a - b).join(', ');

    function report(node, value) {
      context.report({ node, messageId: 'offScale', data: { value: String(value), scale: scaleStr } });
    }

    return {
      'JSXAttribute[name.name="style"] Property[key.name="fontSize"]'(node) {
        const val = node.value;
        if (val.type === 'Literal' && typeof val.value === 'number' && !allowed.has(val.value)) {
          report(val, val.value);
        }
      },
      'JSXAttribute[name.name="className"] Literal'(node) {
        if (typeof node.value !== 'string') return;
        for (const m of node.value.matchAll(ARBITRARY_RE)) {
          const n = Number(m[1]);
          if (!allowed.has(n)) report(node, `${n}px`);
        }
      },
    };
  },
};
