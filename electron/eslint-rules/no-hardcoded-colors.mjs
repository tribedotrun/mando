/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Ban hardcoded colors in inline style objects. Use CSS variable tokens.' },
    messages: {
      hex: 'Hardcoded hex color "{{value}}" in style prop "{{prop}}". Use a var(--*) token.',
      rgb: 'Hardcoded rgb/rgba in style prop "{{prop}}". Use a var(--*) token or color-mix().',
      hsl: 'Hardcoded hsl/hsla in style prop "{{prop}}". Use a var(--*) token or color-mix().',
      named: 'Named color "{{value}}" in style prop "{{prop}}". Use a var(--*) token.',
    },
  },
  create(context) {
    const HEX_RE = /^#[0-9a-f]{3,8}$/i;
    const RGB_RE = /rgba?\s*\(/i;
    const HSL_RE = /hsla?\s*\(/i;
    const NAMED = new Set([
      'white', 'black', 'red', 'blue', 'green', 'orange', 'yellow', 'gray', 'grey',
      'purple', 'pink', 'cyan', 'magenta', 'lime', 'navy', 'teal', 'maroon', 'olive',
      'aqua', 'fuchsia', 'silver',
    ]);
    // CSS functional keywords -- not design-system colors
    const ALLOWED_KEYWORDS = new Set(['transparent', 'currentcolor', 'inherit']);
    const SKIP_PROPS = new Set(['boxShadow', 'textShadow', 'filter']);

    function checkStyleProp(node) {
      if (node.type !== 'Property' || node.key.type !== 'Identifier') return;
      if (SKIP_PROPS.has(node.key.name)) return;
      const val = node.value;
      if (val.type !== 'Literal' || typeof val.value !== 'string') return;
      const s = val.value;
      if (s.startsWith('var(') || s.startsWith('color-mix(')) return;
      if (ALLOWED_KEYWORDS.has(s.toLowerCase())) return;
      const prop = node.key.name;
      if (HEX_RE.test(s)) {
        context.report({ node: val, messageId: 'hex', data: { value: s, prop } });
      } else if (RGB_RE.test(s)) {
        context.report({ node: val, messageId: 'rgb', data: { prop } });
      } else if (HSL_RE.test(s)) {
        context.report({ node: val, messageId: 'hsl', data: { prop } });
      } else if (NAMED.has(s.toLowerCase())) {
        context.report({ node: val, messageId: 'named', data: { value: s, prop } });
      }
    }

    return {
      'JSXAttribute[name.name="style"] ObjectExpression Property'(node) {
        checkStyleProp(node);
      },
    };
  },
};
