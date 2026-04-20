// `throw` is allowed only in three places (per the parse-don't-validate plan):
//   1. Files matching the React-render allowlist (*.tsx).
//   2. The `toReactQuery` translator helper inside #result.
//   3. Statements preceded by a `// invariant: <statement>` comment, marking
//      a programmer-bug throw (not an expected failure).
//
// Anywhere else, fallible code must return Result/ResultAsync instead of throwing.

const REACT_RENDER_ALLOWLIST = /(\.tsx)$|\/global\/repo\/sseCacheHelpers\.ts$/;
const RESULT_HELPERS_ALLOWLIST = /shared\/result\//;
// Files where throwing is the documented error model for now (boundary funnels,
// IPC handlers, lifecycle, codegen). Each will migrate as the plan progresses.
const THROW_ALLOWED_FILES = [
  /\/renderer\/global\/providers\/http\.ts$/,
  /\/main\/global\/runtime\/lifecycle\.ts$/,
  /\/main\/global\/runtime\/ipcSecurity\.ts$/,
  /\/main\/global\/runtime\/launchd\.ts$/,
  /\/main\/global\/runtime\/portCheck\.ts$/,
  /\/main\/global\/runtime\/uiLifecycle\.ts$/,
  /\/main\/global\/service\/launchd\.ts$/,
  /\/main\/onboarding\/repo\/config\.ts$/,
  /\/main\/onboarding\/runtime\/setupValidation\.ts$/,
  /\/main\/updater\/runtime\/updater\.ts$/,
  /\/main\/shell\/runtime\/notifications\.ts$/,
  /\/main\/shell\/runtime\/terminalBridge\.ts$/,
  /\/main\/shell\/runtime\/dock\.ts$/,
  /\/main\/index\.ts$/,
  /\/preload\/providers\/ipc\.ts$/,
  /\/preload\/ipc\/expose\.ts$/,
  /\/renderer\/global\/runtime\/useConfig\.ts$/,
  /\/renderer\/domains\/captain\/repo\/api\.ts$/,
];

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Throw is reserved for programmer-bug invariants, React render, and library-interop translators.',
    },
    messages: {
      bare:
        'Bare throw is banned in non-UI / non-funnel code. Return Err(...) or annotate with `// invariant: <statement>`.',
    },
    schema: [],
  },
  create(context) {
    const filename = context.filename ?? context.getFilename();
    if (REACT_RENDER_ALLOWLIST.test(filename)) return {};
    if (RESULT_HELPERS_ALLOWLIST.test(filename)) return {};
    if (THROW_ALLOWED_FILES.some((re) => re.test(filename))) return {};

    return {
      ThrowStatement(node) {
        const before = context.sourceCode.getCommentsBefore(node);
        if (before.some((c) => /invariant:/i.test(c.value))) return;
        context.report({ node, messageId: 'bare' });
      },
    };
  },
};
