// In `electron/src/main/**`, sync filesystem and shell calls are allowed
// only inside files matching `**/service/**` or in the explicit allowlist
// (a small set of named runtime modules whose entire purpose is to wrap
// the underlying sync IO).
//
// Codifies invariant M4 in .claude/skills/s-arch/invariants.md.

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

// Files that are intentionally sync-IO-shaped. Each entry is a path
// fragment matched against the file's normalized path. Keep the list
// short and intent-clear.
const ALLOWLIST_FRAGMENTS = [
  '/main/global/runtime/launchd.ts',
  '/main/global/runtime/portCheck.ts',
  '/main/global/runtime/appPackage.ts',
  '/main/global/runtime/devGitInfo.ts',
  '/main/global/runtime/appUserDataDir.ts',
  '/main/global/runtime/quitController.ts',
  '/main/global/runtime/trayOwner.ts',
  '/main/onboarding/repo/config.ts',
  '/main/global/runtime/loginItemMigration.ts',
  '/main/shell/runtime/terminalBridge.ts',
  '/main/global/providers/logger.ts',
];

function isMainFile(filename) {
  if (!filename) return false;
  const norm = filename.replaceAll('\\', '/');
  return /(^|\/)src\/main\//.test(norm);
}

function isServiceFile(filename) {
  if (!filename) return false;
  return filename.replaceAll('\\', '/').includes('/service/');
}

function isAllowlisted(filename) {
  if (!filename) return false;
  const norm = filename.replaceAll('\\', '/');
  return ALLOWLIST_FRAGMENTS.some((frag) => norm.endsWith(frag));
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Sync IO calls in main/ are restricted to service modules and a small allowlist of intentionally sync-IO-shaped runtime modules.',
    },
    messages: {
      noSyncIo:
        'Sync IO `{{name}}` in main/ is restricted to service modules or the allowlist (launchd, portCheck, appPackage, devGitInfo, etc.). Move the call behind a named service. See .claude/skills/s-arch/invariants.md#m4.',
    },
  },
  create(context) {
    const filename = context.filename || (context.getFilename && context.getFilename());
    if (!isMainFile(filename)) return {};
    if (isServiceFile(filename)) return {};
    if (isAllowlisted(filename)) return {};

    return {
      CallExpression(node) {
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
          context.report({ node, messageId: 'noSyncIo', data: { name } });
        }
      },
    };
  },
};
