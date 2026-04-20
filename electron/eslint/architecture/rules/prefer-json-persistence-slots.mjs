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
        }
      },
    };
  },
};
