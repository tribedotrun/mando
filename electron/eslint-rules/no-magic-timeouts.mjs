/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Ban raw numeric literals in setTimeout/setInterval. Use named constants.' },
    messages: {
      magic:
        'Magic number {{value}} in {{fn}}(). Extract to a named constant (e.g. const DELAY_MS = {{value}}).',
    },
  },
  create(context) {
    const TIMER_FNS = new Set(['setTimeout', 'setInterval']);

    function check(node) {
      const callee = node.callee;
      let fnName;
      if (callee.type === 'Identifier' && TIMER_FNS.has(callee.name)) {
        fnName = callee.name;
      } else if (
        callee.type === 'MemberExpression' &&
        callee.property.type === 'Identifier' &&
        TIMER_FNS.has(callee.property.name)
      ) {
        fnName = callee.property.name;
      }
      if (!fnName) return;

      // The delay argument is the second argument
      const delayArg = node.arguments[1];
      if (!delayArg) return;

      if (delayArg.type === 'Literal' && typeof delayArg.value === 'number') {
        context.report({
          node: delayArg,
          messageId: 'magic',
          data: { value: String(delayArg.value), fn: fnName },
        });
      }
    }

    return { CallExpression: check };
  },
};
