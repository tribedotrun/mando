// Ban toast imports/calls from `sonner` outside:
// - `**/ui/**` (the UI tier may fire toasts directly),
// - `src/renderer/global/runtime/useFeedback.ts` (single canonical adapter).
//
// Per R9, toasts fire from exactly one funnel; repo/runtime/service tiers
// return errors or results and the UI decides how to surface them. Prevents
// duplicate toasts and inconsistent error UX across tiers.
//
// Codifies invariant R9 in .claude/skills/s-arch/invariants.md.

const ADAPTER_SUFFIX = '/renderer/global/runtime/useFeedback.ts';

function isAllowed(filename) {
  if (filename.includes('/ui/')) return true;
  if (filename.endsWith(ADAPTER_SUFFIX)) return true;
  return false;
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        "Ban toast imports from 'sonner' outside ui/ and the single feedback adapter.",
    },
    messages: {
      noToastOutsideUi:
        "Banned: `toast` import from 'sonner' outside `ui/` and `global/runtime/useFeedback.ts`. Repo/runtime/service tiers must return the result; the UI or the feedback adapter decides how to display. See .claude/skills/s-arch/invariants.md#r9.",
    },
  },
  create(context) {
    const filename = context.filename || context.getFilename();
    if (isAllowed(filename)) return {};

    return {
      ImportDeclaration(node) {
        if (node.source.value !== 'sonner') return;
        for (const spec of node.specifiers) {
          if (spec.type === 'ImportNamespaceSpecifier') {
            context.report({ node: spec, messageId: 'noToastOutsideUi' });
            continue;
          }
          const importedName =
            spec.type === 'ImportSpecifier' &&
            spec.imported &&
            spec.imported.type === 'Identifier'
              ? spec.imported.name
              : spec.type === 'ImportDefaultSpecifier'
                ? 'default'
                : null;
          if (importedName === 'toast' || importedName === 'default') {
            context.report({ node: spec, messageId: 'noToastOutsideUi' });
          }
        }
      },
    };
  },
};
