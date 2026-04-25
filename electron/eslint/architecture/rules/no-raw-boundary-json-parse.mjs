const ALLOWED_FILES = new Set([
  'src/shared/result/helpers.ts',
  'src/shared/result/sse-parse-handler.ts',
]);

function normalize(filename) {
  return filename?.replaceAll('\\', '/') ?? '';
}

function unwrapExpression(node) {
  if (!node) return node;
  if (node.type === 'ChainExpression') return unwrapExpression(node.expression);
  if (node.type === 'SequenceExpression') {
    return node.expressions.length > 0
      ? unwrapExpression(node.expressions[node.expressions.length - 1])
      : node;
  }
  return node;
}

function getPropertyName(node) {
  if (node?.type !== 'MemberExpression') return null;
  if (node.computed) {
    return node.property.type === 'Literal' && typeof node.property.value === 'string'
      ? node.property.value
      : null;
  }
  return node.property.type === 'Identifier' ? node.property.name : null;
}

function isBoundaryFile(filename) {
  const normalized = normalize(filename);
  return (
    /(^|\/)src\/shared\/ipc-contract\//.test(normalized) ||
    /(^|\/)src\/preload\//.test(normalized) ||
    /(^|\/)src\/renderer\//.test(normalized) ||
    /(^|\/)src\/main\/.*\/(repo|providers|runtime|service)\//.test(normalized)
  );
}

function isAllowedFile(filename) {
  const normalized = normalize(filename);
  return Array.from(ALLOWED_FILES).some((allowed) => normalized.endsWith(allowed));
}

function isIdentifier(node, name) {
  return node?.type === 'Identifier' && node.name === name;
}

function getIdentifierName(pattern) {
  if (pattern?.type === 'Identifier') return pattern.name;
  if (pattern?.type === 'AssignmentPattern' && pattern.left.type === 'Identifier') {
    return pattern.left.name;
  }
  return null;
}

function isJsonNamespace(node) {
  if (isIdentifier(node, 'JSON')) return true;
  return (
    node?.type === 'MemberExpression' &&
    getPropertyName(node) === 'JSON' &&
    node.object.type === 'Identifier' &&
    ['globalThis', 'window', 'self'].includes(node.object.name)
  );
}

function isJsonParseReference(node) {
  const callee = unwrapExpression(node);
  return (
    callee?.type === 'MemberExpression' &&
    isJsonNamespace(callee.object) &&
    getPropertyName(callee) === 'parse'
  );
}

function isJsonParseCall(node) {
  return node?.type === 'CallExpression' && isJsonParseReference(node.callee);
}

function isResponseJsonCall(node) {
  const call = unwrapExpression(node);
  return (
    call?.type === 'CallExpression' &&
    call.callee.type === 'MemberExpression' &&
    getPropertyName(call.callee) === 'json'
  );
}

function isJsonParseAlias(node, aliasNames) {
  const expr = unwrapExpression(node);
  return expr?.type === 'Identifier' && aliasNames.has(expr.name);
}

function isJsonParseAliasLike(node, aliasNames) {
  const expr = unwrapExpression(node);
  return isJsonParseReference(expr) || isJsonParseAlias(expr, aliasNames);
}

function isBoundJsonParseFactory(node, aliasNames) {
  const call = unwrapExpression(node);
  return (
    call?.type === 'CallExpression' &&
    call.callee.type === 'MemberExpression' &&
    getPropertyName(call.callee) === 'bind' &&
    isJsonParseAliasLike(call.callee.object, aliasNames)
  );
}

function isJsonParseInvocable(node, aliasNames) {
  const callee = unwrapExpression(node);
  return isJsonParseAliasLike(callee, aliasNames) || isBoundJsonParseFactory(callee, aliasNames);
}

function isJsonParseApplyCall(node, aliasNames) {
  if (node?.type !== 'CallExpression') return false;
  if (node.callee.type === 'MemberExpression') {
    const memberName = getPropertyName(node.callee);
    if (
      (memberName === 'call' || memberName === 'apply') &&
      isJsonParseInvocable(node.callee.object, aliasNames)
    ) {
      return true;
    }
    if (
      memberName === 'apply' &&
      isIdentifier(node.callee.object, 'Reflect') &&
      node.arguments[0] &&
      isJsonParseInvocable(node.arguments[0], aliasNames)
    ) {
      return true;
    }
  }
  return false;
}

function markJsonParseAlias(pattern, aliasNames) {
  const name = getIdentifierName(pattern);
  if (name) aliasNames.add(name);
}

function markJsonParseAliasesFromObjectPattern(pattern, aliasNames) {
  if (pattern?.type !== 'ObjectPattern') return;
  for (const prop of pattern.properties) {
    if (prop.type !== 'Property' || prop.computed) continue;
    const keyName =
      prop.key.type === 'Identifier'
        ? prop.key.name
        : typeof prop.key.value === 'string'
          ? prop.key.value
          : null;
    if (keyName !== 'parse') continue;
    markJsonParseAlias(prop.value, aliasNames);
  }
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Boundary modules must delegate inbound JSON parsing to shared helpers so response bodies stay mechanically auditable.',
    },
    messages: {
      useBoundaryHelper:
        'Boundary modules must parse inbound JSON through shared helpers (parseJsonText, parseJsonTextWith, or parseSseMessage), not raw JSON.parse or response.json(). See .claude/skills/s-arch/invariants.md.',
    },
  },
  create(context) {
    const filename = context.filename || context.getFilename?.();
    if (!isBoundaryFile(filename) || isAllowedFile(filename)) return {};

    const aliasNames = new Set();

    return {
      VariableDeclarator(node) {
        const init = unwrapExpression(node.init);
        if (!init) return;
        if (isJsonParseReference(init) || isBoundJsonParseFactory(init, aliasNames)) {
          markJsonParseAlias(node.id, aliasNames);
          return;
        }
        if (isJsonNamespace(init)) {
          markJsonParseAliasesFromObjectPattern(node.id, aliasNames);
        }
      },
      AssignmentExpression(node) {
        if (node.operator !== '=') return;
        const right = unwrapExpression(node.right);
        if (
          (isJsonParseReference(right) || isBoundJsonParseFactory(right, aliasNames)) &&
          node.left.type === 'Identifier'
        ) {
          aliasNames.add(node.left.name);
          return;
        }
        if (isJsonNamespace(right)) {
          markJsonParseAliasesFromObjectPattern(node.left, aliasNames);
        }
      },
      CallExpression(node) {
        if (
          !isResponseJsonCall(node) &&
          !isJsonParseInvocable(node.callee, aliasNames) &&
          !isJsonParseApplyCall(node, aliasNames)
        ) {
          return;
        }
        context.report({ node, messageId: 'useBoundaryHelper' });
      },
    };
  },
};
