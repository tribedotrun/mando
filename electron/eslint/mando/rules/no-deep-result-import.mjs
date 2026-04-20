// Seals the in-house Result module: only the barrel `#result` may be imported.
// Imports of `#result/internal/*` or relative paths INTO src/shared/result/ from outside
// the module are banned. Mirrors the Rust crate boundary model in JS.

import { resolve } from 'node:path';

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Only the #result barrel may import from src/shared/result/.' },
    messages: {
      deepImport:
        "Deep import '{{path}}' into the Result module is banned. Use `import ... from '#result'`.",
    },
    schema: [],
  },
  create(context) {
    const filename = context.filename ?? context.getFilename();
    const isInsideResult = filename.includes('/src/shared/result/');
    return {
      ImportDeclaration(node) {
        const src = node.source.value;
        if (typeof src !== 'string') return;
        // Block deeper alias imports via #result/...
        if (/^#result\//.test(src)) {
          context.report({ node, messageId: 'deepImport', data: { path: src } });
          return;
        }
        // Block direct alias imports via #shared/result/... (bypasses the barrel)
        if (/^#shared\/result(\/|$)/.test(src)) {
          context.report({ node, messageId: 'deepImport', data: { path: src } });
          return;
        }
        // Block relative imports into the result module from outside
        if (!isInsideResult && /(^|\/)src\/shared\/result($|\/)/.test(resolveSrc(filename, src))) {
          context.report({ node, messageId: 'deepImport', data: { path: src } });
        }
      },
    };
  },
};

function resolveSrc(file, src) {
  if (src.startsWith('.')) {
    return resolve(file, '..', src);
  }
  return src;
}
