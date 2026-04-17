import { isBarrelFile } from '../../../shared/constants.mjs';

const ALLOWED_RE = /\/(runtime|service|types|repo)(\/|$)/;

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Barrels may only re-export from runtime, service, types, and repo.' },
    messages: {
      badExport: 'Barrels may only re-export from runtime, service, types, and repo. Remove UI/config/provider exports. See s-arch skill.',
    },
  },
  create(context) {
    if (!isBarrelFile(context.filename || context.getFilename())) return {};

    return {
      ExportNamedDeclaration(node) {
        if (!node.source) return;
        const source = node.source.value;
        if (!ALLOWED_RE.test(source)) {
          context.report({ node, messageId: 'badExport' });
        }
      },
      ExportAllDeclaration(node) {
        if (!node.source) return;
        const source = node.source.value;
        if (!ALLOWED_RE.test(source)) {
          context.report({ node, messageId: 'badExport' });
        }
      },
    };
  },
};
