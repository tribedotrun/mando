// Ban channel-wide listener cleanup APIs on the preload bridge or main
// process. Subscriptions must return caller-owned disposers. Any
// `removeAllListeners` call or method named like `removeXListeners`
// (plural) indicates the legacy channel-wide cleanup pattern. A singular
// `removeListener(handler)` is a legitimate per-handler API and is not
// flagged.
//
// Codifies invariant B1 in .claude/skills/s-arch/invariants.md.

const REMOVE_LISTENERS_RE = /^remove([A-Z][a-zA-Z]*)?Listeners$/;
const ALL_LISTENERS_RE = /^removeAllListeners$/;

function isMethodNameBanned(name) {
  if (!name) return false;
  if (ALL_LISTENERS_RE.test(name)) return true;
  if (REMOVE_LISTENERS_RE.test(name)) return true;
  return false;
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Ban channel-wide listener cleanup APIs (removeAllListeners, removeXListeners) on preload bridge and renderer callers.',
    },
    messages: {
      noCleanupCall:
        'Banned: channel-wide cleanup `{{name}}()`. Capture the unsubscribe function returned by the on* method and call it from the caller. See .claude/skills/s-arch/invariants.md#b1.',
      noCleanupDecl:
        'Banned: declaring channel-wide cleanup method `{{name}}`. on* methods must return a caller-owned `() => void` disposer instead. See .claude/skills/s-arch/invariants.md#b1.',
    },
  },
  create(context) {
    return {
      // Calls: foo.removeXListeners(), foo.removeAllListeners()
      CallExpression(node) {
        const callee = node.callee;
        if (callee.type !== 'MemberExpression') return;
        if (callee.computed) return;
        if (callee.property.type !== 'Identifier') return;
        const name = callee.property.name;
        if (isMethodNameBanned(name)) {
          context.report({ node, messageId: 'noCleanupCall', data: { name } });
        }
      },
      // Type-level declarations: removeXListeners: () => void
      TSPropertySignature(node) {
        if (node.key.type !== 'Identifier') return;
        if (isMethodNameBanned(node.key.name)) {
          context.report({ node, messageId: 'noCleanupDecl', data: { name: node.key.name } });
        }
      },
      TSMethodSignature(node) {
        if (node.key.type !== 'Identifier') return;
        if (isMethodNameBanned(node.key.name)) {
          context.report({ node, messageId: 'noCleanupDecl', data: { name: node.key.name } });
        }
      },
      // Object literal definitions: removeXListeners: () => { ... }
      Property(node) {
        if (node.computed) return;
        if (node.key.type !== 'Identifier') return;
        if (isMethodNameBanned(node.key.name)) {
          context.report({ node, messageId: 'noCleanupDecl', data: { name: node.key.name } });
        }
      },
    };
  },
};
