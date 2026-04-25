const ALLOWED_FILE = 'src/renderer/global/providers/persistence.ts';
const STRING_SLOT_IMPORTS = new Set([
  'defineSlot',
  'defineKeyspace',
  'PersistedSlot',
  'PersistedKeyspace',
]);

function normalize(filename) {
  return filename?.replaceAll('\\', '/') ?? '';
}

function isAllowedFile(filename) {
  return normalize(filename).endsWith(ALLOWED_FILE);
}

function isJsonMethodCall(node, method) {
  const callee = node.callee;
  return (
    callee?.type === 'MemberExpression' &&
    callee.object.type === 'Identifier' &&
    callee.object.name === 'JSON' &&
    callee.property.type === 'Identifier' &&
    callee.property.name === method
  );
}

function isTrackedStringLiteral(node) {
  return (
    node?.type === 'Literal' &&
    (node.value === '0' || node.value === '1' || node.value === 'false' || node.value === 'true')
  );
}

function isReadCall(node) {
  return (
    node?.type === 'CallExpression' &&
    node.callee?.type === 'MemberExpression' &&
    node.callee.property.type === 'Identifier' &&
    node.callee.property.name === 'read'
  );
}

function isWriteSentinelCall(node) {
  return (
    node.callee?.type === 'MemberExpression' &&
    node.callee.property.type === 'Identifier' &&
    node.callee.property.name === 'write' &&
    node.arguments.length > 0 &&
    isTrackedStringLiteral(node.arguments[0])
  );
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Files using persistence string slots must not hand-roll JSON.parse / JSON.stringify; use defineJsonSlot / defineJsonKeyspace instead.',
    },
    messages: {
      useJsonSlots:
        'Persistence-backed JSON belongs in defineJsonSlot(...) or defineJsonKeyspace(...), not manual JSON. See .claude/skills/s-arch/invariants.md#r16.',
      useTypedSlots:
        'Persistence-backed booleans and structured state belong in defineJsonSlot(...) or defineJsonKeyspace(...), not string sentinels. See .claude/skills/s-arch/invariants.md#r16.',
    },
  },
  create(context) {
    const filename = context.filename || context.getFilename?.();
    if (isAllowedFile(filename)) return {};

    let importsStringSlots = false;

    return {
      ImportDeclaration(node) {
        if (node.source.value !== '#renderer/global/providers/persistence') return;
        importsStringSlots = node.specifiers.some((specifier) => {
          if (specifier.type !== 'ImportSpecifier') return false;
          const imported = specifier.imported;
          return imported.type === 'Identifier' && STRING_SLOT_IMPORTS.has(imported.name);
        });
      },
      CallExpression(node) {
        if (!importsStringSlots) return;
        if (isJsonMethodCall(node, 'parse') || isJsonMethodCall(node, 'stringify')) {
          context.report({ node, messageId: 'useJsonSlots' });
          return;
        }
        if (isWriteSentinelCall(node)) {
          context.report({ node, messageId: 'useTypedSlots' });
        }
      },
      BinaryExpression(node) {
        if (!importsStringSlots) return;
        if (
          (isReadCall(node.left) && isTrackedStringLiteral(node.right)) ||
          (isTrackedStringLiteral(node.left) && isReadCall(node.right))
        ) {
          context.report({ node, messageId: 'useTypedSlots' });
        }
      },
    };
  },
};
