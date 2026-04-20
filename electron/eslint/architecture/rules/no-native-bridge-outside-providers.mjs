function normalize(filename) {
  return filename?.replaceAll('\\', '/') ?? '';
}

function isRendererFile(filename) {
  return normalize(filename).includes('src/renderer/');
}

function isAllowedFile(filename) {
  return normalize(filename).includes('/providers/');
}

function isWindowMandoApi(node) {
  return (
    node?.type === 'MemberExpression' &&
    !node.computed &&
    node.object?.type === 'Identifier' &&
    node.object.name === 'window' &&
    node.property?.type === 'Identifier' &&
    node.property.name === 'mandoAPI'
  );
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Renderer code may access the native bridge only through provider-layer wrappers.',
    },
    messages: {
      noDirectBridge:
        'Direct `window.mandoAPI` access is allowed only in provider boundary modules. Move this call behind a dedicated provider wrapper. See .claude/skills/s-arch/invariants.md#r7.',
    },
  },
  create(context) {
    const filename = context.filename || context.getFilename?.();
    if (!isRendererFile(filename) || isAllowedFile(filename)) return {};

    return {
      MemberExpression(node) {
        if (!isWindowMandoApi(node)) return;
        context.report({ node, messageId: 'noDirectBridge' });
      },
    };
  },
};
