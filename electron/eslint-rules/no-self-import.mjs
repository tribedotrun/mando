import { resolve, dirname } from 'path';

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Ban a module from importing itself.' },
    messages: {
      selfImport: 'Module imports itself. This is likely a mistake.',
    },
  },
  create(context) {
    const filename = context.filename || context.getFilename();

    return {
      ImportDeclaration(node) {
        const source = node.source.value;
        if (!source.startsWith('.')) return;
        const resolved = resolve(dirname(filename), source);
        const withoutExt = filename.replace(/\.[^.]+$/, '');
        if (resolved === filename || resolved === withoutExt) {
          context.report({ node, messageId: 'selfImport' });
        }
      },
    };
  },
};
