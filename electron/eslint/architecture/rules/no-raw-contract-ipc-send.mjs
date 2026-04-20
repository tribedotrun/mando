import fs from 'node:fs';

const ALLOWED_FILE = 'src/main/global/runtime/ipcSecurity.ts';
const CHANNELS_FILE = new URL('../../../src/shared/ipc-contract/channels.ts', import.meta.url);

function normalize(filename) {
  return filename?.replaceAll('\\', '/') ?? '';
}

function loadSubscribeChannels() {
  const source = fs.readFileSync(CHANNELS_FILE, 'utf8');
  const matches = source.matchAll(
    /^\s*(?:'([^']+)'|"([^"]+)"|([A-Za-z_$][\w$]*))\s*:\s*subscribe\(/gm,
  );
  return new Set(
    Array.from(matches, (match) => match[1] ?? match[2] ?? match[3]).filter(Boolean),
  );
}

const SUBSCRIBE_CHANNELS = loadSubscribeChannels();

function isAllowedFile(filename) {
  return normalize(filename).endsWith(ALLOWED_FILE);
}

function isSendCall(node) {
  const callee = node.callee;
  if (!callee || callee.type !== 'MemberExpression') return false;
  if (callee.property.type === 'Identifier' && !callee.computed) {
    return callee.property.name === 'send';
  }
  return (
    callee.property.type === 'Literal' &&
    typeof callee.property.value === 'string' &&
    callee.property.value === 'send'
  );
}

function getLiteralChannel(node) {
  if (!node) return null;
  if (node.type === 'Literal' && typeof node.value === 'string') return node.value;
  if (
    node.type === 'TemplateLiteral' &&
    node.expressions.length === 0 &&
    node.quasis.length === 1
  ) {
    return node.quasis[0]?.value?.cooked ?? null;
  }
  return null;
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Ban raw .send(...) for subscribe channels declared in the shared IPC contract; use sendChannel().',
    },
    messages: {
      useSendChannel:
        'Shared IPC subscribe channel "{{channel}}" must use sendChannel(...), not raw .send(...). See .claude/skills/s-arch/invariants.md#b4.',
    },
  },
  create(context) {
    const filename = context.filename || context.getFilename?.();
    if (isAllowedFile(filename)) return {};

    return {
      CallExpression(node) {
        if (!isSendCall(node)) return;
        const channel = getLiteralChannel(node.arguments[0]);
        if (!channel || !SUBSCRIBE_CHANNELS.has(channel)) return;
        context.report({
          node,
          messageId: 'useSendChannel',
          data: { channel },
        });
      },
    };
  },
};
