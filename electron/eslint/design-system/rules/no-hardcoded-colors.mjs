// Bans hardcoded colors in inline `style={...}` objects AND in Tailwind
// arbitrary-value classNames like `bg-[#fff]` or `text-[rgb(0,0,0)]`.

const HEX_RE = /^#[0-9a-f]{3,8}$/i;
const RGB_RE = /rgba?\s*\(/i;
const HSL_RE = /hsla?\s*\(/i;
const NAMED = new Set([
  'white', 'black', 'red', 'blue', 'green', 'orange', 'yellow', 'gray', 'grey',
  'purple', 'pink', 'cyan', 'magenta', 'lime', 'navy', 'teal', 'maroon', 'olive',
  'aqua', 'fuchsia', 'silver',
]);
const ALLOWED_KEYWORDS = new Set(['transparent', 'currentcolor', 'inherit']);
const SKIP_STYLE_PROPS = new Set(['boxShadow', 'textShadow', 'filter']);

// Tailwind color utility prefixes that take arbitrary values
const COLOR_PREFIXES = ['bg', 'text', 'border', 'fill', 'stroke', 'ring', 'outline', 'shadow', 'from', 'to', 'via', 'divide', 'placeholder', 'caret', 'accent'];
const ARBITRARY_RE = new RegExp(
  `(?:^|\\s)(?:${COLOR_PREFIXES.join('|')})-\\[([^\\]]+)\\]`,
  'gi',
);

function classify(s) {
  if (!s) return null;
  if (s.startsWith('var(') || s.startsWith('color-mix(')) return null;
  if (ALLOWED_KEYWORDS.has(s.toLowerCase())) return null;
  if (HEX_RE.test(s)) return { id: 'hex', value: s };
  if (RGB_RE.test(s)) return { id: 'rgb' };
  if (HSL_RE.test(s)) return { id: 'hsl' };
  if (NAMED.has(s.toLowerCase())) return { id: 'named', value: s };
  return null;
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Ban hardcoded colors in styles or Tailwind arbitrary values.' },
    messages: {
      hex: 'Hardcoded hex color "{{value}}" in {{where}}. Use a var(--*) token.',
      rgb: 'Hardcoded rgb/rgba in {{where}}. Use a var(--*) token or color-mix().',
      hsl: 'Hardcoded hsl/hsla in {{where}}. Use a var(--*) token or color-mix().',
      named: 'Named color "{{value}}" in {{where}}. Use a var(--*) token.',
    },
  },
  create(context) {
    function reportClass(node, hit, where) {
      context.report({ node, messageId: hit.id, data: { value: hit.value || '', where } });
    }

    return {
      'JSXAttribute[name.name="style"] ObjectExpression Property'(node) {
        if (node.key.type !== 'Identifier') return;
        if (SKIP_STYLE_PROPS.has(node.key.name)) return;
        const val = node.value;
        if (val.type !== 'Literal' || typeof val.value !== 'string') return;
        const hit = classify(val.value);
        if (hit) reportClass(val, hit, `style prop "${node.key.name}"`);
      },
      'JSXAttribute[name.name="className"] Literal'(node) {
        if (typeof node.value !== 'string') return;
        for (const m of node.value.matchAll(ARBITRARY_RE)) {
          const hit = classify(m[1]);
          if (hit) reportClass(node, hit, 'Tailwind arbitrary value');
        }
      },
    };
  },
};
