import { isUiFile } from '../../../shared/constants.mjs';

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Ban direct IPC access (window.mandoAPI) inside UI files.' },
    messages: {
      noIpc: 'UI files must not access window.mandoAPI directly. Use runtime hooks. See s-arch skill.',
    },
  },
  create(context) {
    if (!isUiFile(context.filename || context.getFilename())) return {};

    return {
      MemberExpression(node) {
        if (
          node.object.type === 'Identifier' &&
          node.object.name === 'window' &&
          node.property.type === 'Identifier' &&
          node.property.name === 'mandoAPI'
        ) {
          context.report({ node, messageId: 'noIpc' });
        }
      },
    };
  },
};
