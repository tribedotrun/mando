// Renderer-wide ban on module-scope mutable state.
//
// The M3 rule (no-module-scope-mutable-state) is scoped to main-process
// lifecycle.ts. Step D1 extends the principle to every renderer file.
// Targeting `let` alone is insufficient: `const viewHandlers = new Set()` is
// still a shared mutable container at module scope. This rule catches both.
//
// - Bans top-level `let` declarations.
// - Bans top-level `const` bound to `new Set()`, `new Map()`, `new Array()`,
//   `new WeakMap()`, `new WeakSet()`, array literals, and object literals
//   (mutable containers). Primitives and frozen objects are allowed.
//
// Singleton closures remain allowed: `const x = createThing()` is fine because
// the mutable state stays encapsulated inside the function scope instead of
// leaking as a renderer-global bag.
//
// Codifies invariant R11 in .claude/skills/s-arch/invariants.md.

const MUTABLE_CONSTRUCTORS = new Set(['Set', 'Map', 'WeakSet', 'WeakMap', 'Array']);

function isObjectFreezeCall(node) {
  return (
    node &&
    node.type === 'CallExpression' &&
    node.callee.type === 'MemberExpression' &&
    node.callee.object.type === 'Identifier' &&
    node.callee.object.name === 'Object' &&
    node.callee.property.type === 'Identifier' &&
    node.callee.property.name === 'freeze'
  );
}

function unwrapTsWrappers(node) {
  while (
    node &&
    (node.type === 'TSAsExpression' ||
      node.type === 'TSTypeAssertion' ||
      node.type === 'TSSatisfiesExpression')
  ) {
    node = node.expression;
  }
  return node;
}

function isFrozenOrPrimitive(init) {
  if (!init) return true; // no init: bare declaration is frozen-equivalent
  switch (init.type) {
    case 'Literal':
    case 'TemplateLiteral':
      return true;
    case 'UnaryExpression':
      return isFrozenOrPrimitive(init.argument);
    case 'TSAsExpression':
    case 'TSTypeAssertion':
    case 'TSSatisfiesExpression':
      return isFrozenOrPrimitive(init.expression);
    case 'CallExpression': {
      // Object.freeze only counts as immutable when wrapping a plain
      // object/array literal. Object.freeze(new Set()) and friends still
      // expose .add()/.set() — the freeze is a no-op on the inner state.
      if (!isObjectFreezeCall(init)) return false;
      const arg = init.arguments[0];
      if (!arg || arg.type === 'SpreadElement') return false;
      const inner = unwrapTsWrappers(arg);
      return inner.type === 'ObjectExpression' || inner.type === 'ArrayExpression';
    }
    default:
      return false;
  }
}

function isMutableInit(init) {
  if (!init) return false;
  switch (init.type) {
    case 'ArrayExpression':
      return true;
    case 'ObjectExpression':
      return true;
    case 'NewExpression': {
      if (init.callee.type === 'Identifier' && MUTABLE_CONSTRUCTORS.has(init.callee.name)) {
        return true;
      }
      return false;
    }
    case 'CallExpression': {
      // Recurse into Object.freeze(...) so a frozen wrapper around a
      // mutable container (e.g. Object.freeze(new Set())) still counts.
      if (!isObjectFreezeCall(init)) return false;
      const arg = init.arguments[0];
      if (!arg || arg.type === 'SpreadElement') return false;
      return isMutableInit(arg);
    }
    case 'TSAsExpression':
    case 'TSTypeAssertion':
    case 'TSSatisfiesExpression':
      return isMutableInit(init.expression);
    default:
      return false;
  }
}

function describeMutableInit(init) {
  switch (init.type) {
    case 'NewExpression':
      return `new ${init.callee.name}()`;
    case 'ArrayExpression':
      return '[]';
    case 'ObjectExpression':
      return '{}';
    case 'CallExpression': {
      if (isObjectFreezeCall(init) && init.arguments[0]) {
        return `Object.freeze(${describeMutableInit(init.arguments[0])})`;
      }
      return init.type;
    }
    default:
      return init.type;
  }
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description: 'Ban module-scope mutable state in renderer files.',
    },
    messages: {
      noModuleLet:
        'Module-scope `let {{name}}` is banned in the renderer. Encapsulate state in a provider factory, React Query cache, or Zustand store. See .claude/skills/s-arch/invariants.md#r11.',
      noMutableConst:
        'Module-scope `const {{name}} = {{kind}}` is a mutable container. Move it into a closure, React Query cache, or Zustand store. See .claude/skills/s-arch/invariants.md#r11.',
    },
  },
  create(context) {
    const source = context.sourceCode ?? context.getSourceCode?.();

    return {
      VariableDeclaration(node) {
        if (!node.parent || node.parent.type !== 'Program') return;

        for (const decl of node.declarations) {
          const name =
            decl.id.type === 'Identifier'
              ? decl.id.name
              : (source?.getText(decl.id) ?? '<pattern>');

          if (node.kind === 'let') {
            context.report({
              node: decl,
              messageId: 'noModuleLet',
              data: { name },
            });
            continue;
          }

          if (node.kind === 'const') {
            if (isMutableInit(decl.init) && !isFrozenOrPrimitive(decl.init)) {
              const kind = describeMutableInit(decl.init);
              context.report({
                node: decl,
                messageId: 'noMutableConst',
                data: { name, kind },
              });
            }
          }
        }
      },
    };
  },
};
