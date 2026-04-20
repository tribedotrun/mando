// `electron/src/main/index.ts` is composition-only: imports, type
// declarations, top-level constants the bootstrap needs synchronously,
// and a single bootstrap function (`main()` or `app.whenReady().then(...)`).
//
// Bans: module-scope `let` declarations, and `execSync` / `spawnSync` /
// `*Sync` filesystem helpers anywhere in the file (including inside
// callbacks or nested functions). The owners (windowOwner, trayOwner,
// quitController, daemonConnectionState, etc.) hold state and sync IO.
//
// Codifies invariants M1, M2, and (loosely) M3 in
// .claude/skills/s-arch/invariants.md.

const TARGET_FILE_RE = /(^|\/)src\/main\/index\.ts$/;

const SYNC_IO_NAMES = new Set([
  'execSync',
  'spawnSync',
  'execFileSync',
  'readFileSync',
  'writeFileSync',
  'existsSync',
  'mkdirSync',
  'unlinkSync',
  'statSync',
  'readdirSync',
  'rmSync',
  'rmdirSync',
  'cpSync',
]);

// A small allowlist of bare top-level `let` declarations the bootstrap
// genuinely owns at module scope. Everything else must move into an owner.
const ALLOWED_TOP_LEVEL_LETS = new Set([]);

function isApplicable(filename) {
  if (!filename) return false;
  return TARGET_FILE_RE.test(filename.replaceAll('\\', '/'));
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'electron/src/main/index.ts is composition-only: no module-scope `let`, no sync IO calls.',
    },
    messages: {
      noTopLevelLet:
        'electron/src/main/index.ts must not declare module-scope `let {{name}}`. Move the state into the owning module (windowOwner, trayOwner, quitController, daemonConnectionState, ...). See .claude/skills/s-arch/invariants.md#m1.',
      noTopLevelSyncIo:
        'electron/src/main/index.ts must not call sync IO (`{{name}}`). Move the call behind a named service in main/global/runtime/ and call the service from bootstrap. See .claude/skills/s-arch/invariants.md#m4.',
    },
  },
  create(context) {
    const filename = context.filename || (context.getFilename && context.getFilename());
    if (!isApplicable(filename)) return {};

    return {
      VariableDeclaration(node) {
        // Only flag top-level `let` (parent must be Program).
        if (node.kind !== 'let') return;
        if (!node.parent || node.parent.type !== 'Program') return;
        for (const decl of node.declarations) {
          if (decl.id.type !== 'Identifier') continue;
          const name = decl.id.name;
          if (ALLOWED_TOP_LEVEL_LETS.has(name)) continue;
          context.report({ node: decl, messageId: 'noTopLevelLet', data: { name } });
        }
      },
      CallExpression(node) {
        // Flag sync IO calls anywhere in this file. The bootstrap must
        // delegate to an owner module, not call sync IO directly.
        const callee = node.callee;
        let name;
        if (callee.type === 'Identifier') name = callee.name;
        else if (
          callee.type === 'MemberExpression' &&
          !callee.computed &&
          callee.property.type === 'Identifier'
        ) {
          name = callee.property.name;
        }
        if (name && SYNC_IO_NAMES.has(name)) {
          context.report({ node, messageId: 'noTopLevelSyncIo', data: { name } });
        }
      },
    };
  },
};
