// In the remaining stateful main-process owners, top-level `let`
// declarations are banned. Cross-domain lifecycle and updater state must live
// inside typed state containers, not in module-scope `let` bindings.
// `src/main/index.ts` is covered by `main-composition-only` instead, so
// it is intentionally excluded here to avoid double-reporting.
//
// Codifies invariant M3 in .claude/skills/s-arch/invariants.md.

const TARGET_SUFFIXES = ['src/main/global/runtime/lifecycle.ts', 'src/main/updater/runtime/updater.ts'];

function isApplicable(filename) {
  if (!filename) return false;
  const norm = filename.replaceAll('\\', '/');
  return TARGET_SUFFIXES.some((suffix) => norm.endsWith(suffix));
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Stateful main-process owners must not hold module-scope mutable state. Use a typed state container or a pure reducer in a dedicated owner.',
    },
    messages: {
      noModuleLet:
        'Module-scope `let {{name}}` is banned in stateful main-process owners. Move state into a typed container or pure reducer (see daemonConnectionState.ts / updaterState.ts). See .claude/skills/s-arch/invariants.md#m3.',
    },
  },
  create(context) {
    const filename = context.filename || (context.getFilename && context.getFilename());
    if (!isApplicable(filename)) return {};

    return {
      VariableDeclaration(node) {
        if (node.kind !== 'let') return;
        if (!node.parent || node.parent.type !== 'Program') return;
        const source = context.sourceCode ?? context.getSourceCode?.();
        for (const decl of node.declarations) {
          const name =
            decl.id.type === 'Identifier'
              ? decl.id.name
              : (source?.getText(decl.id) ?? '<pattern>');
          context.report({
            node: decl,
            messageId: 'noModuleLet',
            data: { name },
          });
        }
      },
    };
  },
};
