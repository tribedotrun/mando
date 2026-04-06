/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'suggestion',
    docs: { description: 'Ban business logic (Math, parsing, formatting) in component files.' },
    messages: {
      noLogic:
        '"{{name}}" is business logic — extract to utils.ts or a hook. Components should render, not compute.',
    },
  },
  create(context) {
    const filename = context.filename || context.getFilename();
    const isComponent = /\/components\//.test(filename);
    if (!isComponent) return {};

    const BANNED_GLOBALS = new Set(['parseFloat', 'parseInt']);
    const BANNED_MATH = new Set([
      'abs', 'floor', 'ceil', 'round', 'min', 'max', 'pow', 'sqrt', 'log', 'trunc',
    ]);
    const BANNED_METHODS = new Set(['toFixed', 'toPrecision']);

    return {
      CallExpression(node) {
        const c = node.callee;
        // parseFloat(), parseInt()
        if (c.type === 'Identifier' && BANNED_GLOBALS.has(c.name)) {
          context.report({ node, messageId: 'noLogic', data: { name: c.name + '()' } });
        }
        // Math.floor() etc.
        if (c.type === 'MemberExpression' && c.object.type === 'Identifier' &&
            c.object.name === 'Math' && c.property.type === 'Identifier' &&
            BANNED_MATH.has(c.property.name)) {
          context.report({ node, messageId: 'noLogic', data: { name: `Math.${c.property.name}()` } });
        }
        // .toFixed(), .toLocaleString()
        if (c.type === 'MemberExpression' && c.property.type === 'Identifier' &&
            BANNED_METHODS.has(c.property.name)) {
          context.report({ node, messageId: 'noLogic', data: { name: `.${c.property.name}()` } });
        }
      },
      // Number() constructor
      'CallExpression[callee.name="Number"]'(node) {
        context.report({ node, messageId: 'noLogic', data: { name: 'Number()' } });
      },
    };
  },
};
