// Ban toast imports outside:
// - `**/ui/**`,
// - the single feedback adapter,
// - runtime feedback hooks (`**/runtime/useFeedback*.ts[x]`,
//   `**/runtime/useError*.ts[x]`).
//
// Repo/service/providers tiers return data or errors; UI and dedicated runtime
// feedback hooks decide how to surface them. Prevents duplicate toasts and
// keeps the feedback funnel explicit.
//
// Codifies invariant R9 in .claude/skills/s-arch/invariants.md.

const ADAPTER_SUFFIX = '/renderer/global/runtime/useFeedback.ts';
const FEEDBACK_HOOK_RE = /\/runtime\/use(?:Error|Feedback)[^/]*\.tsx?$/;
const TOAST_SOURCES = new Set(['sonner', '#renderer/global/runtime/useFeedback']);

function normalizeFilename(filename) {
  return (filename || '').replaceAll('\\', '/');
}

function isAllowed(filename) {
  const normalized = normalizeFilename(filename);
  if (normalized.includes('/ui/')) return true;
  if (normalized.endsWith(ADAPTER_SUFFIX)) return true;
  return FEEDBACK_HOOK_RE.test(normalized);
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
      description: 'Ban toast imports outside ui/ and dedicated runtime feedback hooks.',
    },
    messages: {
      noToastOutsideUi:
        'Banned: toast imports belong in `ui/`, `global/runtime/useFeedback.ts`, or runtime hooks named `useFeedback*` / `useError*`. Repo/runtime/service/providers tiers must return the result and let those feedback hooks decide how to surface it. See .claude/skills/s-arch/invariants.md#r9.',
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
