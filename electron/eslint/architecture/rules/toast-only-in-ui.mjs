// Ban toast imports outside:
// - `**/ui/**`,
// - renderer runtime hooks (`**/runtime/use*.ts[x]`).
//
// Repo/service/providers tiers return data or errors; UI and runtime hooks
// decide how to surface them. Prevents duplicate toasts while authorizing by
// architectural tier instead of a feedback-oriented filename convention.
//
// Codifies invariant R9 in .claude/skills/s-arch/invariants.md.

const RENDERER_RUNTIME_HOOK_RE = /\/renderer\/.*\/runtime\/use[^/]*\.tsx?$/;
const TOAST_SOURCES = new Set(['sonner', '#renderer/global/runtime/useFeedback']);

function normalizeFilename(filename) {
  return (filename || '').replaceAll('\\', '/');
}

function isAllowed(filename) {
  const normalized = normalizeFilename(filename);
  if (normalized.includes('/ui/')) return true;
  return RENDERER_RUNTIME_HOOK_RE.test(normalized);
}

function isToastSpecifier(spec) {
  if (spec.type === 'ImportNamespaceSpecifier') return true;
  if (spec.type === 'ImportDefaultSpecifier') return true;
  return (
    spec.type === 'ImportSpecifier' &&
    spec.imported &&
    spec.imported.type === 'Identifier' &&
    spec.imported.name === 'toast'
  );
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description: 'Ban toast imports outside renderer ui/ and runtime use-hooks.',
    },
    messages: {
      noToastOutsideUi:
        'Banned: toast imports belong in renderer `ui/` or `runtime/use*` hooks. Repo, service, providers, and non-hook runtime modules must return errors for those surfaces to render. See .claude/skills/s-arch/invariants.md#r9.',
    },
  },
  create(context) {
    const filename = context.filename || context.getFilename();
    if (isAllowed(filename)) return {};

    return {
      ImportDeclaration(node) {
        if (!TOAST_SOURCES.has(node.source.value)) return;
        for (const spec of node.specifiers) {
          if (isToastSpecifier(spec)) {
            context.report({ node: spec, messageId: 'noToastOutsideUi' });
          }
        }
      },
    };
  },
};
