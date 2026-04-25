// Promise chains split success/error/finally policy away from the surrounding
// control flow. In production Electron app code, prefer async/await with
// try/catch/finally so errors are logged or surfaced at the effect boundary.

const PROMISE_CHAIN_METHODS = new Set(['then', 'catch']);
const ALLOWED_PROMISE_INTERNAL_FILES = new Set(['src/shared/result/async-result.ts']);
const ZOD_RUNTIME_PARSE_METHODS = new Set(['parse', 'parseAsync', 'safeParse', 'safeParseAsync', 'spa']);

function normalizedFilename(context) {
  return (context.filename || context.getFilename?.() || '').replaceAll('\\', '/');
}

function isAllowedPromiseInternal(context) {
  const filename = normalizedFilename(context);
  return [...ALLOWED_PROMISE_INTERNAL_FILES].some((allowed) => filename.endsWith(allowed));
}

function collectZodLocalNames(program) {
  const names = new Set();
  const importedNames = new Set();
  for (const node of program.body) {
    if (node.type === 'ImportDeclaration' && node.source.value === 'zod') {
      for (const specifier of node.specifiers) {
        if (specifier.type === 'ImportSpecifier' && specifier.imported?.name === 'z') {
          names.add(specifier.local.name);
          importedNames.add(specifier.local.name);
        }
        if (specifier.type === 'ImportNamespaceSpecifier') {
          names.add(specifier.local.name);
          importedNames.add(specifier.local.name);
        }
      }
      continue;
    }
    if (node.type !== 'VariableDeclaration') continue;
    for (const declaration of node.declarations) {
      if (declaration.id.type !== 'Identifier') continue;
      if (!expressionStartsFromLocal(declaration.init, importedNames)) continue;
      if (hasZodRuntimeParseCall(declaration.init)) continue;
      names.add(declaration.id.name);
    }
  }
  return names;
}

function expressionStartsFromLocal(expression, localNames) {
  if (!expression) return false;
  switch (expression.type) {
    case 'Identifier':
      return localNames.has(expression.name);
    case 'MemberExpression':
      return expressionStartsFromLocal(expression.object, localNames);
    case 'CallExpression':
      return expressionStartsFromLocal(expression.callee, localNames);
    case 'ChainExpression':
      return expressionStartsFromLocal(expression.expression, localNames);
    case 'TSNonNullExpression':
    case 'TSAsExpression':
    case 'TSTypeAssertion':
      return expressionStartsFromLocal(expression.expression, localNames);
    default:
      return false;
  }
}

function isStaticMemberCall(callee) {
  if (callee.type !== 'MemberExpression') return null;
  if (callee.computed) return null;
  if (callee.property.type !== 'Identifier') return null;
  return callee.property.name;
}

function hasZodRuntimeParseCall(expression) {
  if (!expression) return false;
  switch (expression.type) {
    case 'CallExpression': {
      const calleeMethod = isStaticMemberCall(expression.callee);
      if (ZOD_RUNTIME_PARSE_METHODS.has(calleeMethod)) return true;
      return (
        hasZodRuntimeParseCall(expression.callee) ||
        expression.arguments.some((arg) => hasZodRuntimeParseCall(arg))
      );
    }
    case 'MemberExpression':
      return (
        hasZodRuntimeParseCall(expression.object) || hasZodRuntimeParseCall(expression.property)
      );
    case 'ChainExpression':
    case 'TSNonNullExpression':
    case 'TSAsExpression':
    case 'TSTypeAssertion':
      return hasZodRuntimeParseCall(expression.expression);
    default:
      return false;
  }
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description: 'Ban Promise .then/.catch chains in production Electron app code.',
    },
    schema: [],
    messages: {
      promiseChain:
        'Promise .{{method}}() chains are banned in Electron app code. Use async/await with try/catch/finally, or return a typed Result at a boundary API.',
    },
  },
  create(context) {
    if (isAllowedPromiseInternal(context)) return {};

    let zodLocalNames = new Set();

    return {
      Program(node) {
        zodLocalNames = collectZodLocalNames(node);
      },
      CallExpression(node) {
        const method = isStaticMemberCall(node.callee);
        if (!PROMISE_CHAIN_METHODS.has(method)) return;

        // Zod's `.catch(...)` is a schema combinator, not Promise error handling.
        if (method === 'catch' && expressionStartsFromLocal(node.callee.object, zodLocalNames)) {
          return;
        }

        context.report({
          node: node.callee.property,
          messageId: 'promiseChain',
          data: { method },
        });
      },
    };
  },
};
