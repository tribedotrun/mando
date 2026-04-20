// Every `eslint-disable*` comment must include `-- reason: <why>`. Forces every
// escape hatch to leave an audit trail explaining the bypass.

const DISABLE_RE = /^\s*eslint-disable(?:-next-line|-line)?(?:\s|$)/;

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'suggestion',
    docs: { description: 'eslint-disable* comments must include `-- reason: <why>`.' },
    messages: {
      missing: 'eslint-disable comment must include `-- reason: <why>` to document the bypass.',
    },
    schema: [],
  },
  create(context) {
    return {
      Program() {
        const comments = context.sourceCode.getAllComments();
        for (const c of comments) {
          if (!DISABLE_RE.test(c.value)) continue;
          // Accept either `-- reason: <why>` (preferred) or any `-- <text>` after the rule list.
          if (!/--\s*\S+/.test(c.value)) {
            context.report({ loc: c.loc, messageId: 'missing' });
          }
        }
      },
    };
  },
};
