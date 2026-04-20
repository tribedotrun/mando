const CREATOR_NAMES = new Set([
  'useQuery',
  'useSuspenseQuery',
  'useInfiniteQuery',
  'useSuspenseInfiniteQuery',
  'useMutation',
]);

function normalize(filename) {
  return filename?.replaceAll('\\', '/') ?? '';
}

function isRendererFile(filename) {
  return normalize(filename).includes('src/renderer/');
}

function isRepoFile(filename) {
  return normalize(filename).includes('/repo/');
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'React Query creator hooks belong in renderer repo modules only. Runtime and app compose repo hooks; they do not create alternate query layers.',
    },
    messages: {
      noCreator:
        'React Query creator `{{name}}` belongs in a repo module. Move the hook construction into repo/ and let runtime consume it. See .claude/skills/s-arch/invariants.md#r6.',
    },
  },
  create(context) {
    const filename = context.filename || context.getFilename?.();
    if (!isRendererFile(filename) || isRepoFile(filename)) return {};

    return {
      ImportDeclaration(node) {
        if (node.source.value !== '@tanstack/react-query') return;
        for (const specifier of node.specifiers) {
          if (specifier.type !== 'ImportSpecifier') continue;
          const importedName =
            specifier.imported.type === 'Identifier' ? specifier.imported.name : null;
          if (!importedName || !CREATOR_NAMES.has(importedName)) continue;
          context.report({
            node: specifier,
            messageId: 'noCreator',
            data: { name: importedName },
          });
        }
      },
    };
  },
};
