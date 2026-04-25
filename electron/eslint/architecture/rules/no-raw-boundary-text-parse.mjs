function normalize(filename) {
  return filename?.replaceAll('\\', '/') ?? '';
}

function isMainBoundaryFile(filename) {
  const normalized = normalize(filename);
  return /(^|\/)src\/main\/.*\/(repo|providers|service|runtime)\//.test(normalized);
}

function unwrapExpression(node) {
  if (!node) return node;
  if (node.type === 'AwaitExpression') return unwrapExpression(node.argument);
  if (node.type === 'ChainExpression') return unwrapExpression(node.expression);
  return node;
}

function getPropertyName(node) {
  if (node.type !== 'MemberExpression') return null;
  if (node.computed) {
    return node.property.type === 'Literal' && typeof node.property.value === 'string'
      ? node.property.value
      : null;
  }
  return node.property.type === 'Identifier' ? node.property.name : null;
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

function isMemberCall(node, methodName) {
  return (
    node?.type === 'CallExpression' &&
    node.callee.type === 'MemberExpression' &&
    getPropertyName(node.callee) === methodName
  );
}

function isParseIntCall(node) {
  return node?.type === 'CallExpression' && isIdentifier(node.callee, 'parseInt');
}

function isNumberCall(node) {
  return node?.type === 'CallExpression' && isIdentifier(node.callee, 'Number');
}

function isFsReadCall(node) {
  const call = unwrapExpression(node);
  if (call?.type !== 'CallExpression') return false;

  const callee = call.callee;
  if (isIdentifier(callee, 'readFileSync') || isIdentifier(callee, 'readFile')) return true;
  if (callee.type !== 'MemberExpression') return false;

  const directObject = callee.object;
  const propertyName = getPropertyName(callee);
  if (
    isIdentifier(directObject, 'fs') &&
    (propertyName === 'readFileSync' || propertyName === 'readFile')
  ) {
    return true;
  }

  return (
    directObject.type === 'MemberExpression' &&
    isIdentifier(directObject.object, 'fs') &&
    getPropertyName(directObject) === 'promises' &&
    propertyName === 'readFile'
  );
}

function isExecSyncCall(node) {
  const call = unwrapExpression(node);
  if (call?.type !== 'CallExpression') return false;
  const callee = call.callee;
  return (
    isIdentifier(callee, 'execSync') ||
    isIdentifier(callee, 'execFileSync') ||
    getPropertyName(callee) === 'execSync' ||
    getPropertyName(callee) === 'execFileSync'
  );
}

function isStderrStringCall(node) {
  const call = unwrapExpression(node);
  return call?.type === 'CallExpression' && isIdentifier(call.callee, 'stderrString');
}

const COMMAND_RESULT_NAMES = new Set([
  'run',
  'execAsync',
  'execFileAsync',
  'execa',
  'execaCommand',
  'spawnAsync',
]);

function isCommandResultCall(node) {
  const call = unwrapExpression(node);
  if (call?.type !== 'CallExpression') return false;
  if (call.callee.type === 'Identifier') {
    return COMMAND_RESULT_NAMES.has(call.callee.name);
  }
  return COMMAND_RESULT_NAMES.has(getPropertyName(call.callee));
}

function isResponseTextCall(node) {
  const call = unwrapExpression(node);
  return call?.type === 'CallExpression' && getPropertyName(call.callee) === 'text';
}

function markIdentifier(pattern, store) {
  const name = getIdentifierName(pattern);
  if (name) store.add(name);
}

function markStdoutBindings(pattern, store) {
  if (pattern?.type !== 'ObjectPattern') return;
  for (const prop of pattern.properties) {
    if (prop.type !== 'Property' || prop.computed) continue;
    const keyName =
      prop.key.type === 'Identifier'
        ? prop.key.name
        : typeof prop.key.value === 'string'
          ? prop.key.value
          : null;
    if (keyName !== 'stdout') continue;
    markIdentifier(prop.value, store);
  }
}

function isTrackedStdout(node, commandResultVars) {
  const expr = unwrapExpression(node);
  if (expr?.type !== 'MemberExpression' || getPropertyName(expr) !== 'stdout') return false;
  return (
    isCommandResultCall(expr.object) ||
    (expr.object.type === 'Identifier' && commandResultVars.has(expr.object.name))
  );
}

function isBoundaryTextExpression(node, boundaryTextVars, commandResultVars) {
  const expr = unwrapExpression(node);
  if (!expr) return false;
  if (
    isFsReadCall(expr) ||
    isExecSyncCall(expr) ||
    isStderrStringCall(expr) ||
    isResponseTextCall(expr)
  ) {
    return true;
  }
  if (expr.type === 'Identifier' && boundaryTextVars.has(expr.name)) return true;
  if (isTrackedStdout(expr, commandResultVars)) return true;
  return false;
}

function isBoundaryParseIntArg(node, boundaryTextVars, commandResultVars) {
  const expr = unwrapExpression(node);
  return (
    isBoundaryTextExpression(expr, boundaryTextVars, commandResultVars) ||
    (isMemberCall(expr, 'trim') &&
      isBoundaryTextExpression(expr.callee.object, boundaryTextVars, commandResultVars))
  );
}

function isBoundaryRegexExecCall(node, boundaryTextVars, commandResultVars) {
  return (
    isMemberCall(node, 'exec') &&
    node.arguments[0] &&
    isBoundaryTextExpression(node.arguments[0], boundaryTextVars, commandResultVars)
  );
}

function isBoundaryStringMethodCall(node, methodName, boundaryTextVars, commandResultVars) {
  return (
    isMemberCall(node, methodName) &&
    isBoundaryTextExpression(node.callee.object, boundaryTextVars, commandResultVars)
  );
}

function isParamBoundaryExpression(node, paramNames) {
  const expr = unwrapExpression(node);
  return expr?.type === 'Identifier' && paramNames.has(expr.name);
}

function isParamParseIntArg(node, paramNames) {
  const expr = unwrapExpression(node);
  return (
    isParamBoundaryExpression(expr, paramNames) ||
    (isMemberCall(expr, 'trim') && isParamBoundaryExpression(expr.callee.object, paramNames))
  );
}

function isParamNumberArg(node, paramNames) {
  return isParamBoundaryExpression(unwrapExpression(node), paramNames);
}

function isParamBoundaryOperation(node, paramNames) {
  if (isMemberCall(node, 'trim') && isParamBoundaryExpression(node.callee.object, paramNames)) {
    return true;
  }
  if (isMemberCall(node, 'match') && isParamBoundaryExpression(node.callee.object, paramNames)) {
    return true;
  }
  if (
    isMemberCall(node, 'exec') &&
    node.arguments[0] &&
    isParamBoundaryExpression(node.arguments[0], paramNames)
  ) {
    return true;
  }
  if (
    (isMemberCall(node, 'replace') || isMemberCall(node, 'replaceAll')) &&
    isParamBoundaryExpression(node.callee.object, paramNames)
  ) {
    return true;
  }
  if (
    isParseIntCall(node) &&
    node.arguments[0] &&
    isParamParseIntArg(node.arguments[0], paramNames)
  ) {
    return true;
  }
  if (isNumberCall(node) && node.arguments[0] && isParamNumberArg(node.arguments[0], paramNames)) {
    return true;
  }
  return false;
}

function visitNode(node, visitor) {
  if (!node || typeof node !== 'object') return;
  if (visitor(node) === false) return;
  for (const [key, value] of Object.entries(node)) {
    if (key === 'parent' || key === 'loc' || key === 'range') continue;
    if (Array.isArray(value)) {
      for (const child of value) visitNode(child, visitor);
      continue;
    }
    if (value && typeof value === 'object' && 'type' in value) {
      visitNode(value, visitor);
    }
  }
}

function collectBoundaryHelperNames(ast) {
  const helperNames = new Set();

  function maybeAddHelper(name, fnNode) {
    if (!name) return;
    const paramNames = new Set(
      fnNode.params.map((param) => getIdentifierName(param)).filter(Boolean),
    );
    if (paramNames.size === 0) return;

    let found = false;
    visitNode(fnNode.body, (child) => {
      if (found) return false;
      if (
        child !== fnNode &&
        ['FunctionDeclaration', 'FunctionExpression', 'ArrowFunctionExpression'].includes(
          child.type,
        )
      ) {
        return false;
      }
      if (child.type === 'CallExpression' && isParamBoundaryOperation(child, paramNames)) {
        found = true;
        return false;
      }
      return undefined;
    });

    if (found) helperNames.add(name);
  }

  visitNode(ast, (node) => {
    if (node.type === 'FunctionDeclaration') {
      maybeAddHelper(node.id?.name ?? null, node);
      return false;
    }
    if (
      node.type === 'VariableDeclarator' &&
      node.id.type === 'Identifier' &&
      node.init &&
      ['FunctionExpression', 'ArrowFunctionExpression'].includes(node.init.type)
    ) {
      maybeAddHelper(node.id.name, node.init);
      return false;
    }
    return undefined;
  });

  return helperNames;
}

function callbackUsesBoundaryParam(node) {
  if (!node || !['FunctionExpression', 'ArrowFunctionExpression'].includes(node.type)) {
    return false;
  }

  const paramNames = new Set(node.params.map((param) => getIdentifierName(param)).filter(Boolean));
  if (paramNames.size === 0) return false;

  let found = false;
  const body = node.body.type === 'BlockStatement' ? node.body : node.body;
  visitNode(body, (child) => {
    if (found) return false;
    if (
      child !== node &&
      ['FunctionDeclaration', 'FunctionExpression', 'ArrowFunctionExpression'].includes(child.type)
    ) {
      return false;
    }
    if (child.type === 'CallExpression' && isParamBoundaryOperation(child, paramNames)) {
      found = true;
      return false;
    }
    return undefined;
  });

  return found;
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Main-process boundary modules must parse file text and command output through shared Zod-backed helpers instead of ad hoc trim/regex/parseInt logic.',
    },
    messages: {
      useBoundaryTextHelper:
        'Boundary text from files or command output must parse through shared Zod-backed helpers, not raw trim/regex/parseInt logic. See .claude/skills/s-arch/invariants.md.',
    },
  },
  create(context) {
    const filename = context.filename || context.getFilename?.();
    if (!isMainBoundaryFile(filename)) return {};

    const sourceCode = context.sourceCode ?? context.getSourceCode?.();
    const boundaryTextHelperNames = collectBoundaryHelperNames(sourceCode.ast);
    const boundaryTextVars = new Set();
    const commandResultVars = new Set();

    return {
      VariableDeclarator(node) {
        const init = unwrapExpression(node.init);
        if (!init) return;

        if (isCommandResultCall(init)) {
          markIdentifier(node.id, commandResultVars);
          markStdoutBindings(node.id, boundaryTextVars);
          return;
        }

        if (
          isTrackedStdout(init, commandResultVars) ||
          isBoundaryTextExpression(init, boundaryTextVars, commandResultVars)
        ) {
          markIdentifier(node.id, boundaryTextVars);
          return;
        }

        if (init.type === 'Identifier' && commandResultVars.has(init.name)) {
          markStdoutBindings(node.id, boundaryTextVars);
        }
      },
      AssignmentExpression(node) {
        if (node.operator !== '=') return;
        const right = unwrapExpression(node.right);
        if (isCommandResultCall(right)) {
          if (node.left.type === 'Identifier') {
            commandResultVars.add(node.left.name);
            return;
          }
          markStdoutBindings(node.left, boundaryTextVars);
          return;
        }

        if (
          node.left.type === 'Identifier' &&
          isBoundaryTextExpression(right, boundaryTextVars, commandResultVars)
        ) {
          boundaryTextVars.add(node.left.name);
          return;
        }

        if (right?.type === 'Identifier' && commandResultVars.has(right.name)) {
          markStdoutBindings(node.left, boundaryTextVars);
        }
      },
      CallExpression(node) {
        if (
          isBoundaryStringMethodCall(node, 'trim', boundaryTextVars, commandResultVars)
        ) {
          context.report({ node, messageId: 'useBoundaryTextHelper' });
          return;
        }

        if (
          isBoundaryStringMethodCall(node, 'match', boundaryTextVars, commandResultVars)
        ) {
          context.report({ node, messageId: 'useBoundaryTextHelper' });
          return;
        }

        if (isBoundaryRegexExecCall(node, boundaryTextVars, commandResultVars)) {
          context.report({ node, messageId: 'useBoundaryTextHelper' });
          return;
        }

        if (
          isBoundaryStringMethodCall(node, 'replace', boundaryTextVars, commandResultVars) ||
          isBoundaryStringMethodCall(node, 'replaceAll', boundaryTextVars, commandResultVars)
        ) {
          context.report({ node, messageId: 'useBoundaryTextHelper' });
          return;
        }

        if (
          node.callee.type === 'Identifier' &&
          boundaryTextHelperNames.has(node.callee.name) &&
          node.arguments.some((arg) =>
            isBoundaryTextExpression(arg, boundaryTextVars, commandResultVars),
          )
        ) {
          context.report({ node, messageId: 'useBoundaryTextHelper' });
          return;
        }

        if (
          isParseIntCall(node) &&
          node.arguments[0] &&
          isBoundaryParseIntArg(node.arguments[0], boundaryTextVars, commandResultVars)
        ) {
          context.report({ node, messageId: 'useBoundaryTextHelper' });
          return;
        }

        if (
          isNumberCall(node) &&
          node.arguments[0] &&
          isBoundaryTextExpression(node.arguments[0], boundaryTextVars, commandResultVars)
        ) {
          context.report({ node, messageId: 'useBoundaryTextHelper' });
          return;
        }

        if (
          isMemberCall(node, 'then') &&
          isBoundaryTextExpression(node.callee.object, boundaryTextVars, commandResultVars) &&
          node.arguments.some((arg) => callbackUsesBoundaryParam(arg))
        ) {
          context.report({ node, messageId: 'useBoundaryTextHelper' });
        }
      },
    };
  },
};
