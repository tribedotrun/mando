// Bans borderRadius values outside design tokens, in inline styles AND in
// Tailwind arbitrary classNames like `rounded-[7px]`.

const DEFAULT_RADII = [4, 6, 8, 10];
const ARBITRARY_RE = /(?:^|\s)rounded(?:-[a-z]+)?-\[([^\]]+)\]/g;

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Ban borderRadius values outside the design tokens.' },
    schema: [
      {
        type: 'object',
        properties: { allowed: { type: 'array', items: { type: 'number' } } },
        additionalProperties: false,
      },
    ],
    messages: {
      offGrid: 'borderRadius {{value}} is not a token. Allowed: {{tokens}} or "50%".',
    },
  },
  create(context) {
    const allowed = new Set(context.options[0]?.allowed || DEFAULT_RADII);
    const tokensStr = [...allowed].sort((a, b) => a - b).join(', ');

    function report(node, value) {
      context.report({ node, messageId: 'offGrid', data: { value: String(value), tokens: tokensStr } });
    }

    function isAllowedString(s) {
      if (s === '50%' || s.startsWith('var(')) return true;
      const m = s.match(/^(\d+(?:\.\d+)?)px$/);
      if (m && allowed.has(Number(m[1]))) return true;
      return false;
    }

    return {
      'JSXAttribute[name.name="style"] Property[key.name="borderRadius"]'(node) {
        const val = node.value;
        if (val.type !== 'Literal') return;
        if (typeof val.value === 'number' && !allowed.has(val.value)) {
          report(val, val.value);
        } else if (typeof val.value === 'string' && !isAllowedString(val.value)) {
          report(val, val.value);
        }
      },
      'JSXAttribute[name.name="className"] Literal'(node) {
        if (typeof node.value !== 'string') return;
        for (const m of node.value.matchAll(ARBITRARY_RE)) {
          if (!isAllowedString(m[1])) report(node, m[1]);
        }
      },
    };
  },
};
