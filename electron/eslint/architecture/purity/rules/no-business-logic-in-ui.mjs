import { isUiFile } from '../../../shared/constants.mjs';

const BANNED_GLOBALS = new Set(['parseFloat', 'parseInt', 'Number']);
const BANNED_MATH = new Set([
  'abs', 'floor', 'ceil', 'round', 'min', 'max', 'pow', 'sqrt', 'log', 'trunc',
]);
const BANNED_METHODS = new Set(['toFixed', 'toPrecision']);

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'suggestion',
    docs: { description: 'Ban math/parsing/formatting calls inside UI files.' },
    messages: {
      noLogic: '"{{name}}" is business logic. Extract to a service or hook. See s-arch skill.',
    },
  },
  create(context) {
    if (!isUiFile(context.filename || context.getFilename())) return {};

    function report(node, name) {
      context.report({ node, messageId: 'noLogic', data: { name } });
    }

    return {
      CallExpression(node) {
        const c = node.callee;
        if (c.type === 'Identifier' && BANNED_GLOBALS.has(c.name)) {
          report(node, `${c.name}()`);
          return;
        }
        if (c.type !== 'MemberExpression' || c.property.type !== 'Identifier') return;
        if (
          c.object.type === 'Identifier' &&
          c.object.name === 'Math' &&
          BANNED_MATH.has(c.property.name)
        ) {
          report(node, `Math.${c.property.name}()`);
        } else if (BANNED_METHODS.has(c.property.name)) {
          report(node, `.${c.property.name}()`);
        }
      },
    };
  },
};
