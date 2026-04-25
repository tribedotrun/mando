const MAX_KEYS = 8;
const HOOK_NAME_RE = /^use[A-Z0-9]/;

function isRendererFile(filename) {
  return (filename ?? '').replaceAll('\\', '/').includes('src/renderer/');
}

function isRepoFile(filename) {
  return (filename ?? '').replaceAll('\\', '/').includes('/repo/');
}

function walk(node, visitor) {
  if (!node || typeof node.type !== 'string') return;
  if (visitor(node) === 'skip') return;
  for (const [key, value] of Object.entries(node)) {
    if (
      key === 'parent' ||
      key === 'loc' ||
      key === 'range' ||
      key === 'tokens' ||
      key === 'comments'
    )
      continue;
    if (Array.isArray(value)) {
      for (const child of value) walk(child, visitor);
    } else if (value && typeof value.type === 'string') {
      walk(value, visitor);
    }
  }
}

function isFunctionLike(node) {
  return (
    node?.type === 'FunctionDeclaration' ||
    node?.type === 'FunctionExpression' ||
    node?.type === 'ArrowFunctionExpression'
  );
}

function walkOwnBody(node, visitor) {
  walk(node, (child) => {
    if (child !== node && isFunctionLike(child)) return 'skip';
    visitor(child);
  });
}

function functionName(node) {
  if (node.type === 'FunctionDeclaration') return node.id?.name;
  const parent = node.parent;
  if (parent?.type === 'VariableDeclarator' && parent.id.type === 'Identifier')
    return parent.id.name;
  return null;
}

function isExported(node) {
  let cur = node.parent;
  while (cur) {
    if (cur.type === 'ExportNamedDeclaration') return true;
    if (cur.type === 'Program') return false;
    cur = cur.parent;
  }
  return false;
}

function keyName(prop) {
  if (prop.type === 'SpreadElement') return '...spread';
  if (prop.key?.type === 'Identifier') return prop.key.name;
  if (prop.key?.type === 'Literal') return String(prop.key.value);
  return '<computed>';
}

function returnObjectExpressions(node) {
  const returns = [];
  if (node.body?.type === 'ObjectExpression') {
    returns.push(node.body);
    return returns;
  }
  walkOwnBody(node.body, (child) => {
    if (child.type === 'ReturnStatement' && child.argument?.type === 'ObjectExpression') {
      returns.push(child.argument);
    }
  });
  return returns;
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Exported renderer hooks should not return god-bag objects.' },
    messages: {
      tooMany:
        '`{{name}}` returns {{count}} top-level fields (max {{max}}). Split responsibilities or return named sub-hooks/action families. See .claude/skills/s-arch/invariants.md#r23.',
      opaqueSpread:
        '`{{name}}` returns an object spread whose arity is hidden. Return explicit fields or split the hook. See .claude/skills/s-arch/invariants.md#r23.',
    },
  },
  create(context) {
    const filename = context.filename || context.getFilename?.();
    if (!isRendererFile(filename)) return {};
    if (isRepoFile(filename)) return {};

    function checkFunction(node) {
      const name = functionName(node);
      if (!HOOK_NAME_RE.test(name ?? '') || !isExported(node)) return;
      let maxCount = 0;
      let opaqueSpread = null;
      let reportNode = node;
      for (const returnedObject of returnObjectExpressions(node)) {
        reportNode = returnedObject;
        let count = 0;
        for (const prop of returnedObject.properties) {
          if (prop.type === 'SpreadElement') {
            opaqueSpread = prop;
            count += MAX_KEYS + 1;
          } else {
            count += 1;
          }
        }
        maxCount = Math.max(maxCount, count);
      }
      if (opaqueSpread) {
        context.report({ node: opaqueSpread, messageId: 'opaqueSpread', data: { name } });
        return;
      }
      if (maxCount > MAX_KEYS) {
        context.report({
          node: reportNode,
          messageId: 'tooMany',
          data: { name, count: maxCount, max: MAX_KEYS },
        });
      }
    }

    return {
      FunctionDeclaration: checkFunction,
      ArrowFunctionExpression: checkFunction,
      FunctionExpression: checkFunction,
    };
  },
};
