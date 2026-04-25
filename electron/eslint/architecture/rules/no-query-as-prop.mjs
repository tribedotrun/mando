import { readFileSync } from 'node:fs';

const registry = JSON.parse(
  readFileSync(new URL('../query-registry.generated.json', import.meta.url), 'utf8'),
);
const REGISTERED_HOOKS = new Set(registry.hooks ?? []);
const CONTROL_PROPS = new Set(['children', 'className', 'style', 'key', 'ref']);
const META_PROPS = new Set([
  'page',
  'pages',
  'totalPages',
  'filter',
  'filterStatus',
  'filterCategory',
  'value',
  'checked',
  'disabled',
  'status',
  'error',
  'loading',
  'pending',
  'health',
  'askReopenState',
  'history',
]);

function isRendererFile(filename) {
  return (filename ?? '').replaceAll('\\', '/').includes('src/renderer/');
}

function isAllowedValueName(name) {
  return /^(is|has|can|should|show)[A-Z]/.test(name ?? '');
}

function isAllowedProp(propName) {
  if (!propName) return true;
  if (CONTROL_PROPS.has(propName) || META_PROPS.has(propName)) return true;
  if (/^(id|.*Id|.*ID|.*Key)$/.test(propName)) return true;
  if (/^on[A-Z]/.test(propName)) return true;
  return false;
}

function walk(node, visitor) {
  if (!node || typeof node.type !== 'string') return;
  visitor(node);
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

function patternIdentifiers(pattern, into = []) {
  if (!pattern) return into;
  if (pattern.type === 'Identifier') into.push(pattern);
  if (pattern.type === 'RestElement') patternIdentifiers(pattern.argument, into);
  if (pattern.type === 'AssignmentPattern') patternIdentifiers(pattern.left, into);
  if (pattern.type === 'ArrayPattern') {
    for (const element of pattern.elements) patternIdentifiers(element, into);
  }
  if (pattern.type === 'ObjectPattern') {
    for (const property of pattern.properties) {
      if (property.type === 'Property') patternIdentifiers(property.value, into);
      if (property.type === 'RestElement') patternIdentifiers(property.argument, into);
    }
  }
  return into;
}

function functionParams(node) {
  return node?.params ?? [];
}

function callName(node) {
  if (node?.type !== 'CallExpression') return null;
  return node.callee?.type === 'Identifier' ? node.callee.name : null;
}

function isFunctionLike(node) {
  return (
    node?.type === 'FunctionDeclaration' ||
    node?.type === 'FunctionExpression' ||
    node?.type === 'ArrowFunctionExpression'
  );
}

function containsTracked(node, isTracked) {
  const localScopes = [new Set()];
  let found = false;

  function localScope() {
    return localScopes[localScopes.length - 1];
  }

  function declareLocal(pattern) {
    for (const ident of patternIdentifiers(pattern)) localScope().add(ident.name);
  }

  function isLocal(name) {
    return localScopes.some((scope) => scope.has(name));
  }

  function visit(child) {
    if (found || !child || typeof child.type !== 'string') return;

    if (isFunctionLike(child)) {
      localScopes.push(new Set());
      for (const param of functionParams(child)) declareLocal(param);
      visit(child.body);
      localScopes.pop();
      return;
    }

    if (child.type === 'VariableDeclarator') {
      visit(child.init);
      declareLocal(child.id);
      return;
    }

    if (child.type === 'Identifier') {
      if (!isLocal(child.name) && isTracked(child.name)) found = true;
      return;
    }

    if (child.type === 'MemberExpression') {
      visit(child.object);
      if (child.computed) visit(child.property);
      return;
    }

    if (child.type === 'Property') {
      if (child.computed) visit(child.key);
      visit(child.value);
      return;
    }

    for (const [key, value] of Object.entries(child)) {
      if (
        key === 'parent' ||
        key === 'loc' ||
        key === 'range' ||
        key === 'tokens' ||
        key === 'comments'
      )
        continue;
      if (Array.isArray(value)) {
        for (const nested of value) visit(nested);
      } else if (value && typeof value.type === 'string') {
        visit(value);
      }
    }
  }

  visit(node);
  return found;
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Server-backed query payloads should not cross JSX boundaries as props.' },
    messages: {
      queryProp:
        '`{{name}}` comes from a server-state hook and is passed as prop `{{prop}}`. Pass identity/filter props and let the leaf read through a runtime query hook. See .claude/skills/s-arch/invariants.md#r22.',
    },
  },
  create(context) {
    const filename = context.filename || context.getFilename?.();
    if (!isRendererFile(filename)) return {};
    const localHooks = new Set(REGISTERED_HOOKS);
    const scopes = [{ declared: new Set(), tracked: new Set() }];

    function currentScope() {
      return scopes[scopes.length - 1];
    }

    function declarePattern(pattern, tracked = false) {
      for (const ident of patternIdentifiers(pattern)) {
        currentScope().declared.add(ident.name);
        if (tracked) currentScope().tracked.add(ident.name);
      }
    }

    function isTracked(name) {
      for (let i = scopes.length - 1; i >= 0; i -= 1) {
        if (scopes[i].tracked.has(name)) return true;
        if (scopes[i].declared.has(name)) return false;
      }
      return false;
    }

    function enterFunction(node) {
      scopes.push({ declared: new Set(), tracked: new Set() });
      for (const param of functionParams(node)) declarePattern(param);
    }

    function exitFunction() {
      scopes.pop();
    }

    return {
      ImportDeclaration(node) {
        for (const specifier of node.specifiers) {
          if (specifier.type !== 'ImportSpecifier') continue;
          const imported =
            specifier.imported.type === 'Identifier' ? specifier.imported.name : null;
          if (imported && REGISTERED_HOOKS.has(imported)) localHooks.add(specifier.local.name);
        }
      },
      FunctionDeclaration: enterFunction,
      'FunctionDeclaration:exit': exitFunction,
      FunctionExpression: enterFunction,
      'FunctionExpression:exit': exitFunction,
      ArrowFunctionExpression: enterFunction,
      'ArrowFunctionExpression:exit': exitFunction,
      VariableDeclarator(node) {
        const name = callName(node.init);
        if (name && localHooks.has(name)) {
          declarePattern(node.id, true);
          return;
        }
        declarePattern(node.id, Boolean(node.init && containsTracked(node.init, isTracked)));
      },
      JSXAttribute(node) {
        const propName = node.name?.name;
        if (isAllowedProp(propName)) return;
        const expr = node.value?.type === 'JSXExpressionContainer' ? node.value.expression : null;
        if (!expr || expr.type !== 'Identifier' || !isTracked(expr.name)) return;
        if (isAllowedValueName(expr.name)) return;
        context.report({
          node: expr,
          messageId: 'queryProp',
          data: { name: expr.name, prop: propName },
        });
      },
    };
  },
};
