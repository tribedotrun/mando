const COMPONENT_NAME_RE = /^[A-Z]/;
const HOOK_NAME_RE = /^use[A-Z0-9]/;
const TARGET_SUFFIX_RE = /(Form|Input|Composer)$/;
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
const ALLOWED_PROP_NAMES = new Set([
  'children',
  'className',
  'formClassName',
  'formStyle',
  'placeholder',
  'ref',
  'scrollRef',
  'style',
  'testId',
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

function containsJsx(node) {
  let found = false;
  walk(node.body ?? node, (child) => {
    if (child.type === 'JSXElement' || child.type === 'JSXFragment') found = true;
  });
  return found;
}

function functionName(node) {
  if (node.type === 'FunctionDeclaration') return node.id?.name;
  const parent = node.parent;
  if (parent?.type === 'VariableDeclarator' && parent.id.type === 'Identifier')
    return parent.id.name;
  return null;
}

function unwrapExpression(node) {
  let cur = node;
  while (
    cur &&
    (cur.type === 'TSAsExpression' ||
      cur.type === 'TSTypeAssertion' ||
      cur.type === 'ChainExpression' ||
      cur.type === 'ParenthesizedExpression' ||
      cur.type === 'TSNonNullExpression')
  ) {
    cur =
      cur.type === 'ChainExpression' || cur.type === 'ParenthesizedExpression'
        ? cur.expression
        : cur.type === 'TSNonNullExpression'
          ? cur.expression
          : cur.expression;
  }
  return cur;
}

function memberChain(node) {
  const cur = unwrapExpression(node);
  if (!cur) return null;
  if (cur.type === 'Identifier') return [cur.name];
  if (cur.type !== 'MemberExpression' || cur.computed) return null;
  const objectChain = memberChain(cur.object);
  if (!objectChain) return null;
  if (cur.property.type === 'Identifier') return [...objectChain, cur.property.name];
  if (cur.property.type === 'Literal') return [...objectChain, String(cur.property.value)];
  return null;
}

function jsxName(node) {
  if (node?.type === 'JSXIdentifier') return node.name;
  return null;
}

function isControlPropName(name) {
  return /^(on[A-Z]|set[A-Z]|handle[A-Z]|do[A-Z]|remove[A-Z]|cancel[A-Z]|submit[A-Z])/.test(
    name ?? '',
  );
}

function isControlChain(chain) {
  const tail = chain.at(-1) ?? '';
  return /^(set[A-Z]|handle[A-Z]|do[A-Z]|remove[A-Z]|cancel[A-Z]|submit[A-Z])/.test(tail);
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Leaf form/input/composer components should own their local draft and mutation state instead of receiving a hook-owned control bag.',
    },
    messages: {
      leafBag:
        '`{{source}}` only funnels leaf-local state into `{{component}}`. Move that state/mutation ownership into the leaf or pass a stable identity/control prop instead. See .claude/skills/s-arch/invariants.md#r28.',
    },
  },
  create(context) {
    const filename = context.filename || context.getFilename?.();
    if (!isRendererFile(filename) || isPrimitiveUiFile(filename)) return {};

    function checkFunction(node) {
      const name = functionName(node);
      if (!isComponentName(name) || !containsJsx(node)) return;

      /** @type {Set<string>} */
      const trackedRoots = new Set();
      walk(node.body, (child) => {
        if (child.type !== 'VariableDeclarator') return;
        if (child.id.type !== 'Identifier') return;
        if (!isHookCall(child.init)) return;
        trackedRoots.add(child.id.name);
      });
      if (trackedRoots.size === 0) return;

      /** @type {Map<string, number>} */
      const chainUseCounts = new Map();
      walk(node.body, (child) => {
        if (child.type !== 'MemberExpression') return;
        const chain = memberChain(child);
        if (!chain || !trackedRoots.has(chain[0])) return;
        const key = chain.join('.');
        chainUseCounts.set(key, (chainUseCounts.get(key) ?? 0) + 1);
      });

      walk(node.body, (child) => {
        if (child.type !== 'JSXOpeningElement') return;
        const component = jsxName(child.name);
        if (!component || !TARGET_SUFFIX_RE.test(component)) return;

        /** @type {Map<string, Array<{ attr: any, chain: string[], propName: string }>>} */
        const forwardedByRoot = new Map();

        for (const attr of child.attributes) {
          if (attr.type !== 'JSXAttribute') continue;
          const propName = attr.name?.name;
          if (typeof propName !== 'string' || ALLOWED_PROP_NAMES.has(propName)) continue;
          const expr = attr.value?.type === 'JSXExpressionContainer' ? attr.value.expression : null;
          const chain = memberChain(expr);
          if (!chain || !trackedRoots.has(chain[0])) continue;
          const list = forwardedByRoot.get(chain[0]) ?? [];
          list.push({ attr, chain, propName });
          forwardedByRoot.set(chain[0], list);
        }

        for (const [root, forwarded] of forwardedByRoot) {
          const forwardedOnly = forwarded.filter(
            ({ chain }) => (chainUseCounts.get(chain.join('.')) ?? 0) === 1,
          );
          if (forwardedOnly.length < 4) continue;

          const hasControl = forwardedOnly.some(
            ({ propName, chain }) => isControlPropName(propName) || isControlChain(chain),
          );
          const hasState = forwardedOnly.some(
            ({ propName, chain }) =>
              !isControlPropName(propName) &&
              !isControlChain(chain) &&
              !/^(id|.*Id|.*ID|.*Key)$/.test(propName),
          );
          if (!hasControl || !hasState) continue;

          const sourceChains = forwardedOnly.map(({ chain }) => chain);
          let common = [...sourceChains[0]];
          for (const chain of sourceChains.slice(1)) {
            let i = 0;
            while (i < common.length && i < chain.length && common[i] === chain[i]) i += 1;
            common = common.slice(0, i);
          }
          const source = common.length >= 2 ? common.join('.') : root;

          context.report({
            node: forwardedOnly[0].attr,
            messageId: 'leafBag',
            data: { source, component },
          });
          break;
        }
      });
    }

    return {
      FunctionDeclaration: checkFunction,
      ArrowFunctionExpression: checkFunction,
      FunctionExpression: checkFunction,
    };
  },
};
