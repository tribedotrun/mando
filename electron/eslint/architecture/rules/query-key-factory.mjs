// Every React Query `queryKey` and `mutationKey` literal must come from
// the centralized factory in #renderer/global/repo/queryKeys (or a
// `[...factoryEntry, ...modifier]` extension). Inline string-array
// literals create cache identities that no other code can reach.
//
// Codifies invariant R1 in .claude/skills/s-arch/invariants.md.

function isFactoryFile(filename) {
  if (!filename) return false;
  return filename.replaceAll('\\', '/').endsWith('global/repo/queryKeys.ts');
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'queryKey / mutationKey must come from #renderer/global/repo/queryKeys; no inline string-array literals.',
    },
    messages: {
      inlineKey:
        'Inline {{prop}} literal. Add an entry to #renderer/global/repo/queryKeys (queryKeys.<domain>.<name>) and reference it here. See .claude/skills/s-arch/invariants.md#r1.',
    },
  },
  create(context) {
    const filename = context.filename || (context.getFilename && context.getFilename());
    if (isFactoryFile(filename)) return {};

    function isStringElement(el) {
      if (!el) return false;
      if (el.type === 'Literal' && typeof el.value === 'string') return true;
      // Both static ``tasks`` and dynamic ``tasks-${id}`` bypass the factory,
      // so flag template literals regardless of expression count.
      if (el.type === 'TemplateLiteral') return true;
      return false;
    }

    function check(node) {
      const propName = node.key.type === 'Identifier' ? node.key.name : node.key.value;
      if (propName !== 'queryKey' && propName !== 'mutationKey') return;
      const value = node.value;
      if (!value || value.type !== 'ArrayExpression') return;
      // First element of the key tuple identifies the cache namespace; if it's
      // an inline string literal, the key was assembled by hand instead of via
      // the factory.
      const first = value.elements[0];
      if (isStringElement(first)) {
        context.report({ node, messageId: 'inlineKey', data: { prop: propName } });
      }
    }

    return {
      Property: check,
    };
  },
};
