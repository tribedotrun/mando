const ALLOWED_FILES = new Set([
  'src/shared/result/helpers.ts',
  'src/shared/result/sse-parse-handler.ts',
]);

function normalize(filename) {
  return filename?.replaceAll('\\', '/') ?? '';
}

function isBoundaryFile(filename) {
  const normalized = normalize(filename);
  return (
    /(^|\/)src\/shared\/ipc-contract\//.test(normalized) ||
    /(^|\/)src\/(main|renderer)\/.*\/(repo|providers|runtime|service)\//.test(normalized)
  );
}

function isAllowedFile(filename) {
  const normalized = normalize(filename);
  return Array.from(ALLOWED_FILES).some((allowed) => normalized.endsWith(allowed));
}

function isJsonParseCall(node) {
  const callee = node.callee;
  return (
    callee?.type === 'MemberExpression' &&
    callee.object.type === 'Identifier' &&
    callee.object.name === 'JSON' &&
    callee.property.type === 'Identifier' &&
    callee.property.name === 'parse'
  );
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Boundary modules must delegate JSON.parse to shared helpers so inbound JSON parsing stays mechanically auditable.',
    },
    messages: {
      useBoundaryHelper:
        'Boundary modules must parse JSON through shared helpers (parseJsonText, parseJsonTextWith, or parseSseMessage), not raw JSON.parse. See .claude/skills/s-arch/invariants.md#r19.',
    },
  },
  create(context) {
    const filename = context.filename || context.getFilename?.();
    if (!isBoundaryFile(filename) || isAllowedFile(filename)) return {};

    return {
      CallExpression(node) {
        if (!isJsonParseCall(node)) return;
        context.report({ node, messageId: 'useBoundaryHelper' });
      },
    };
  },
};
