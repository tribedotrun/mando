const COMPONENT_NAME_RE = /^[A-Z]/;
const HOOK_NAME_RE = /^use[A-Z0-9]/;
const BUILTIN_HOOKS = new Set([
  'useCallback',
  'useContext',
  'useDebugValue',
  'useDeferredValue',
  'useEffect',
  'useId',
  'useImperativeHandle',
  'useInsertionEffect',
  'useLayoutEffect',
  'useMemo',
  'useOptimistic',
  'useReducer',
  'useRef',
  'useState',
  'useSyncExternalStore',
  'useTransition',
]);
const ALLOWED_NAMES = new Set([
  'children',
  'className',
  'style',
  'key',
  'ref',
  'variant',
  'size',
  'title',
  'label',
  'history',
]);

function normalized(filename) {
  return (filename ?? '').replaceAll('\\', '/');
}

function isRendererFile(filename) {
  return normalized(filename).includes('src/renderer/');
}

function isPrimitiveUiFile(filename) {
  return normalized(filename).includes('src/renderer/global/ui/primitives/');
}

function isComponentName(name) {
  return Boolean(name && COMPONENT_NAME_RE.test(name));
}

function isHookCall(node) {
  if (node?.type !== 'CallExpression' || node.callee?.type !== 'Identifier') return false;
  const name = node.callee.name;
  if (name === 'useState' || name === 'useReducer') return true;
  return HOOK_NAME_RE.test(name) && !BUILTIN_HOOKS.has(name);
}

function isAllowedPropName(name, source) {
  if (!name) return true;
  if (ALLOWED_NAMES.has(name)) return true;
  if (/^(id|.*Id|.*ID|.*Key)$/.test(name)) return true;
  if (
    /^(on[A-Z]|set[A-Z]|handle[A-Z]|open[A-Z]|close[A-Z]|toggle[A-Z]|do[A-Z]|find[A-Z]|remove[A-Z]|cancel[A-Z]|start[A-Z])/.test(
      name,
    )
  )
    return true;
  if (/(Ref|Mut|ClassName|Style)$/.test(name)) return true;
  if (source === 'prop' && /^(on[A-Z]|set[A-Z])/.test(name)) return true;
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

function isFunctionLike(node) {
  return (
    node?.type === 'FunctionDeclaration' ||
    node?.type === 'FunctionExpression' ||
    node?.type === 'ArrowFunctionExpression'
  );
}

function walkOwnBody(node, visitor, root = node) {
  if (!node || typeof node.type !== 'string') return;
  if (node !== root && isFunctionLike(node)) return;
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
      for (const child of value) walkOwnBody(child, visitor, root);
    } else if (value && typeof value.type === 'string') {
      walkOwnBody(value, visitor, root);
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

function functionName(node) {
  if (node.type === 'FunctionDeclaration') return node.id?.name;
  const parent = node.parent;
  if (parent?.type === 'VariableDeclarator' && parent.id.type === 'Identifier')
    return parent.id.name;
  return null;
}

function soleReturnedJsxElement(node) {
  const returns = [];
  walkOwnBody(node.body, (child) => {
    if (child.type === 'ReturnStatement') returns.push(child.argument);
  });
  if (returns.length !== 1) return null;
  const arg = returns[0];
  if (arg?.type === 'JSXElement') return arg;
  if (arg?.type === 'ParenthesizedExpression' && arg.expression?.type === 'JSXElement')
    return arg.expression;
  return null;
}

function jsxChildElementCount(element) {
  if (!element || element.type !== 'JSXElement') return 0;
  return element.children.filter(
    (child) => child.type === 'JSXElement' || child.type === 'JSXFragment',
  ).length;
}

function containsJsx(node) {
  let found = false;
  walk(node.body ?? node, (child) => {
    if (child.type === 'JSXElement' || child.type === 'JSXFragment') found = true;
  });
  return found;
}

function isDeclarationIdentifier(node) {
  const parent = node.parent;
  if (!parent) return false;
  if (parent.type === 'VariableDeclarator' && parent.id === node) return true;
  if (
    (parent.type === 'FunctionDeclaration' || parent.type === 'FunctionExpression') &&
    parent.id === node
  )
    return true;
  if (parent.type === 'Property' && parent.key === node && parent.parent?.type === 'ObjectPattern')
    return true;
  if (
    parent.type === 'Property' &&
    parent.value === node &&
    parent.parent?.type === 'ObjectPattern'
  )
    return true;
  if (parent.type === 'ArrayPattern') return true;
  if (parent.type === 'RestElement' && parent.argument === node) return true;
  if (parent.type === 'AssignmentPattern' && parent.left === node) return true;
  if (parent.type === 'ImportSpecifier' || parent.type === 'ImportDefaultSpecifier') return true;
  return false;
}

function isJsxForwardValue(node) {
  const parent = node.parent;
  return (
    parent?.type === 'JSXExpressionContainer' &&
    parent.parent?.type === 'JSXAttribute' &&
    parent.expression === node
  );
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Components should not read hook/state values only to forward them unchanged to one child.',
    },
    messages: {
      mailbox:
        '`{{name}}` is only forwarded to one child. Move the read/state to that leaf or pass a stable identity instead. See .claude/skills/s-arch/invariants.md#r21.',
    },
  },
  create(context) {
    const filename = context.filename || context.getFilename?.();
    if (!isRendererFile(filename) || isPrimitiveUiFile(filename)) return {};
    if ((filename ?? '').replaceAll('\\', '/').includes('src/renderer/global/ui/')) return {};

    function checkFunction(node) {
      const name = functionName(node);
      if (!isComponentName(name) || !containsJsx(node)) return;
      const returnedElement = soleReturnedJsxElement(node);
      if (!returnedElement || jsxChildElementCount(returnedElement) !== 0) return;

      const tracked = new Map();

      walk(node.body, (child) => {
        if (child.type !== 'VariableDeclarator') return;
        if (!isHookCall(child.init)) return;
        for (const ident of patternIdentifiers(child.id)) {
          if (!isAllowedPropName(ident.name, 'hook')) tracked.set(ident.name, 'hook');
        }
      });

      for (const [identName, source] of [...tracked]) {
        let forwarded = null;
        let forwardCount = 0;
        let otherCount = 0;
        walk(node.body, (child) => {
          if (child.type !== 'Identifier' || child.name !== identName) return;
          if (isDeclarationIdentifier(child)) return;
          if (isJsxForwardValue(child)) {
            const attr = child.parent.parent;
            const propName = attr.name?.name;
            if (isAllowedPropName(propName, source)) {
              otherCount += 1;
              return;
            }
            forwarded = child;
            forwardCount += 1;
            return;
          }
          otherCount += 1;
        });
        if (forwardCount === 1 && otherCount === 0 && forwarded) {
          context.report({ node: forwarded, messageId: 'mailbox', data: { name: identName } });
        }
      }
    }

    return {
      FunctionDeclaration: checkFunction,
      ArrowFunctionExpression: checkFunction,
      FunctionExpression: checkFunction,
    };
  },
};
