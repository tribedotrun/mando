// Renderer files must not touch `localStorage`, `sessionStorage`, or
// `window.localStorage` / `globalThis.localStorage` directly. The single
// allowed boundary is `#renderer/global/providers/persistence`. Codifies
// invariant R3 in .claude/skills/s-arch/invariants.md.

const BANNED_NAMES = new Set(['localStorage', 'sessionStorage']);

function isAllowedFile(filename) {
  if (!filename) return false;
  const norm = filename.replaceAll('\\', '/');
  return norm.endsWith('src/renderer/global/providers/persistence.ts');
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Ban direct localStorage / sessionStorage references in renderer files; route through #renderer/global/providers/persistence.',
    },
    messages: {
      noDirect:
        '{{name}} is banned in renderer code. Use #renderer/global/providers/persistence (defineSlot / defineKeyspace / createPrefixedStorage). See .claude/skills/s-arch/invariants.md#r3.',
    },
  },
  create(context) {
    const filename = context.filename || (context.getFilename && context.getFilename());
    if (isAllowedFile(filename)) return {};

    const sourceCode = context.sourceCode ?? context.getSourceCode?.();

    function isGlobalRef(node, name) {
      if (!BANNED_NAMES.has(name)) return false;
      // Resolve to the enclosing scope. If the name resolves to a local
      // binding (parameter, let/const, function, import), it shadows the
      // global and is safe.
      const scope = sourceCode?.getScope
        ? sourceCode.getScope(node)
        : (context.getScope && context.getScope());
      if (!scope) return true;
      let current = scope;
      while (current) {
        const v = current.set?.get?.(name);
        if (v && (v.defs?.length ?? 0) > 0) return false;
        current = current.upper;
      }
      return true;
    }

    return {
      Identifier(node) {
        // Skip property names (e.g. obj.localStorage where localStorage is just
        // a property name, not a global reference). Reporting on the *root*
        // identifier reference is enough to catch real usage.
        const parent = node.parent;
        if (
          parent &&
          parent.type === 'MemberExpression' &&
          parent.property === node &&
          !parent.computed
        ) {
          return;
        }
        if (isGlobalRef(node, node.name)) {
          context.report({ node, messageId: 'noDirect', data: { name: node.name } });
        }
      },
      MemberExpression(node) {
        // Catch `window.localStorage`, `globalThis.localStorage`,
        // `window['localStorage']`, etc.
        if (node.object.type !== 'Identifier') return;
        const objectName = node.object.name;
        if (objectName !== 'window' && objectName !== 'globalThis' && objectName !== 'self') {
          return;
        }
        let propName;
        if (node.property.type === 'Identifier' && !node.computed) {
          propName = node.property.name;
        } else if (node.property.type === 'Literal' && typeof node.property.value === 'string') {
          propName = node.property.value;
        }
        if (propName && BANNED_NAMES.has(propName)) {
          context.report({ node, messageId: 'noDirect', data: { name: propName } });
        }
      },
    };
  },
};
