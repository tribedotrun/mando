import { isServiceFile } from '../../../shared/constants.mjs';

const BANNED_IMPORTS = [
  { pattern: /^react$/, name: 'React' },
  { pattern: /^react\//, name: 'React internals' },
  { pattern: /^react-dom/, name: 'React DOM' },
  { pattern: /\/providers\//, name: 'providers' },
  { pattern: /\/runtime\//, name: 'runtime' },
  { pattern: /\/repo\//, name: 'repo' },
  { pattern: /\/ui\//, name: 'UI' },
];

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Service files must be pure: no React, no providers, no IPC, no runtime imports.' },
    messages: {
      impure: 'Service files must be pure: no {{name}} imports. Move I/O to repo, move hooks to runtime. See s-arch skill.',
    },
  },
  create(context) {
    if (!isServiceFile(context.filename || context.getFilename())) return {};

    return {
      ImportDeclaration(node) {
        const source = node.source.value;
        for (const { pattern, name } of BANNED_IMPORTS) {
          if (pattern.test(source)) {
            context.report({ node, messageId: 'impure', data: { name } });
            return;
          }
        }
      },
    };
  },
};
