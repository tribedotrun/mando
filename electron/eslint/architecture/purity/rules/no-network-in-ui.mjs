import { isUiFile } from '../../../shared/constants.mjs';

const BANNED_IMPORT_PATTERNS = [
  { pattern: /\/providers\/http/, messageId: 'noHttpProvider' },
  { pattern: /^@tanstack\/react-query$/, messageId: 'noReactQuery' },
];

const BANNED_SPECIFIERS = new Set(['apiGet', 'apiPost', 'apiPatch', 'apiDel', 'apiPut']);

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Ban network access inside UI files: no fetch(), no HTTP provider imports, no direct react-query.' },
    messages: {
      noFetch: 'UI files must not call fetch() directly. Use repo hooks via runtime. See s-arch skill.',
      noHttpProvider: 'UI files must not import HTTP providers (apiGet/apiPost/etc). Use repo hooks via runtime. See s-arch skill.',
      noReactQuery: 'UI files must not import from @tanstack/react-query directly. Use hooks from runtime/. See s-arch skill.',
      noHttpFn: 'UI files must not import "{{name}}" (HTTP function). Move data access to repo, expose via runtime hooks. See s-arch skill.',
    },
  },
  create(context) {
    if (!isUiFile(context.filename || context.getFilename())) return {};

    return {
      'CallExpression[callee.name="fetch"]'(node) {
        context.report({ node, messageId: 'noFetch' });
      },
      'CallExpression[callee.property.name="fetch"]'(node) {
        context.report({ node, messageId: 'noFetch' });
      },
      ImportDeclaration(node) {
        const source = node.source.value;
        for (const { pattern, messageId } of BANNED_IMPORT_PATTERNS) {
          if (pattern.test(source)) {
            context.report({ node, messageId });
            return;
          }
        }
        for (const spec of node.specifiers) {
          if (spec.type === 'ImportSpecifier' && BANNED_SPECIFIERS.has(spec.imported.name)) {
            context.report({ node, messageId: 'noHttpFn', data: { name: spec.imported.name } });
            return;
          }
        }
      },
    };
  },
};
