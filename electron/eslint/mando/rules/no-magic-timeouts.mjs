// Bans raw numeric literals in setTimeout/setInterval delays.
// `setTimeout(fn, 0)` is idiomatic React deferral and is allowed by default.
// `setInterval(fn, 0)` is intentionally NOT exempt because it creates a
// busy-loop polling pattern. Override per call site with the `allow` option.

const TIMER_FNS = new Set(['setTimeout', 'setInterval']);
const DEFAULT_ALLOW_BY_TIMER = { setTimeout: new Set([0]), setInterval: new Set() };

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Ban magic numbers in setTimeout/setInterval delays.' },
    schema: [
      {
        type: 'object',
        properties: {
          // Applied to BOTH setTimeout and setInterval. Use this to add
          // project-wide constants like 16 for one-frame.
          allow: { type: 'array', items: { type: 'number' } },
        },
        additionalProperties: false,
      },
    ],
    messages: {
      magic: 'Magic number {{value}} in {{fn}}(). Extract to a named constant (e.g. const DELAY_MS = {{value}}).',
    },
  },
  create(context) {
    const userAllow = new Set(context.options[0]?.allow ?? []);

    function isAllowed(fn, value) {
      if (userAllow.has(value)) return true;
      return DEFAULT_ALLOW_BY_TIMER[fn].has(value);
    }

    function timerName(callee) {
      if (callee.type === 'Identifier' && TIMER_FNS.has(callee.name)) return callee.name;
      if (
        callee.type === 'MemberExpression' &&
        callee.property.type === 'Identifier' &&
        TIMER_FNS.has(callee.property.name)
      ) {
        return callee.property.name;
      }
      return null;
    }

    return {
      CallExpression(node) {
        const fn = timerName(node.callee);
        if (!fn) return;
        const delay = node.arguments[1];
        if (!delay || delay.type !== 'Literal' || typeof delay.value !== 'number') return;
        if (isAllowed(fn, delay.value)) return;
        context.report({
          node: delay,
          messageId: 'magic',
          data: { value: String(delay.value), fn },
        });
      },
    };
  },
};
