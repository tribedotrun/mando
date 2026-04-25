const MAX_FIELDS = 3;

function isRendererFile(filename) {
  return (filename ?? '').replaceAll('\\', '/').includes('src/renderer/');
}

function isStoreName(name) {
  return /^use[A-Z0-9].*Store$/.test(name ?? '');
}

function countSelectorReads(selector) {
  const param = selector.params?.[0];
  if (!param || param.type !== 'Identifier') return 0;
  const name = param.name;
  const props = new Set();
  function walk(node) {
    if (!node || typeof node.type !== 'string') return;
    if (
      node.type === 'MemberExpression' &&
      node.object?.type === 'Identifier' &&
      node.object.name === name &&
      !node.computed &&
      node.property?.type === 'Identifier'
    ) {
      props.add(node.property.name);
    }
    for (const [key, value] of Object.entries(node)) {
      if (key === 'parent' || key === 'loc' || key === 'range') continue;
      if (Array.isArray(value)) {
        for (const child of value) walk(child);
      } else if (value && typeof value.type === 'string') {
        walk(value);
      }
    }
  }
  walk(selector.body);
  return props.size;
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Zustand consumers should use fine-grained selectors.' },
    messages: {
      bare: '`{{name}}()` destructures {{count}} fields. Use per-field selectors like `{{name}}(s => s.foo)`. See .claude/skills/s-arch/invariants.md#r24.',
      selector:
        '`{{name}}` selector reads {{count}} fields. Split into fine-grained selectors at the leaf. See .claude/skills/s-arch/invariants.md#r24.',
    },
  },
  create(context) {
    const filename = context.filename || context.getFilename?.();
    if (!isRendererFile(filename)) return {};
    const stores = new Set();

    return {
      ImportSpecifier(node) {
        if (isStoreName(node.local?.name)) stores.add(node.local.name);
      },
      VariableDeclarator(node) {
        if (node.id.type === 'Identifier' && isStoreName(node.id.name)) stores.add(node.id.name);
        const init = node.init;
        if (init?.type !== 'CallExpression' || init.callee?.type !== 'Identifier') return;
        if (!stores.has(init.callee.name) && !isStoreName(init.callee.name)) return;
        if (node.id.type === 'ObjectPattern' && node.id.properties.length > MAX_FIELDS) {
          context.report({
            node: node.id,
            messageId: 'bare',
            data: { name: init.callee.name, count: node.id.properties.length },
          });
        }
      },
      CallExpression(node) {
        if (node.callee?.type !== 'Identifier') return;
        const storeName = node.callee.name;
        if (!stores.has(storeName) && !isStoreName(storeName)) return;
        const selector = node.arguments?.[0];
        if (selector?.type !== 'ArrowFunctionExpression' && selector?.type !== 'FunctionExpression')
          return;
        const count = countSelectorReads(selector);
        if (count > MAX_FIELDS) {
          context.report({
            node: selector,
            messageId: 'selector',
            data: { name: storeName, count },
          });
        }
      },
    };
  },
};
