function normalize(filename) {
  return filename?.replaceAll('\\', '/') ?? '';
}

function isRendererFile(filename) {
  return normalize(filename).includes('src/renderer/');
}

function isReloadTarget(node) {
  if (!node || node.type !== 'MemberExpression' || node.computed) return false;
  if (node.property.type !== 'Identifier' || node.property.name !== 'reload') return false;

  const target = node.object;
  if (target.type === 'Identifier' && target.name === 'location') return true;

  return (
    target.type === 'MemberExpression' &&
    !target.computed &&
    target.object.type === 'Identifier' &&
    (target.object.name === 'window' || target.object.name === 'document') &&
    target.property.type === 'Identifier' &&
    target.property.name === 'location'
  );
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Renderer recovery flows must use explicit reset/invalidate/reconnect logic instead of full-page reloads.',
    },
    messages: {
      noReload:
        'Full page reload is banned in renderer recovery flows. Use an explicit reset, invalidate, or reconnect path instead. See .claude/skills/s-arch/invariants.md#r8.',
    },
  },
  create(context) {
    const filename = context.filename || context.getFilename?.();
    if (!isRendererFile(filename)) return {};

    return {
      CallExpression(node) {
        if (!isReloadTarget(node.callee)) return;
        context.report({ node, messageId: 'noReload' });
      },
    };
  },
};
