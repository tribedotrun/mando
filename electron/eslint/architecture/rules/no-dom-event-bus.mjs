// Ban CustomEvent + dispatchEvent + custom-named addEventListener for
// feature-to-feature coordination. Only OS-level event listeners
// (`keydown`, `mousemove`, `mouseup`, `mousedown`, `keyup`, `pointerdown`,
// `pointermove`, `pointerup`, `wheel`, `resize`) are allowed -- those are
// genuine OS boundary subscriptions, not a feature event bus.
//
// Codifies invariant R2 in .claude/skills/s-arch/invariants.md.

const ALLOWED_EVENT_NAMES = new Set([
  'keydown',
  'keyup',
  'keypress',
  'mousemove',
  'mousedown',
  'mouseup',
  'click',
  'pointerdown',
  'pointermove',
  'pointerup',
  'wheel',
  'resize',
  'beforeunload',
  'focus',
  'blur',
  'visibilitychange',
  // PR #883 invariant #1: browser-level error channels. These are genuine
  // OS / runtime boundary subscriptions — not feature-to-feature coordination
  // — and are required for the renderer-side uncaught-error logging path.
  'error',
  'unhandledrejection',
]);

const BUS_RECEIVER_NAMES = new Set(['window', 'document', 'globalThis', 'self']);

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Ban DOM CustomEvent buses for feature coordination in the renderer. Use typed pub/sub modules under global/providers or zustand stores.',
    },
    messages: {
      noCustomEvent:
        'Banned: `new CustomEvent(...)` for feature-to-feature coordination. Use a typed pub/sub module (see global/providers/viewBriefBus, obsHealth) or a Zustand action. See .claude/skills/s-arch/invariants.md#r2.',
      noDispatchEvent:
        'Banned: `dispatchEvent(...)` on window/document for feature coordination. Use a typed pub/sub module or a Zustand action. See .claude/skills/s-arch/invariants.md#r2.',
      noCustomListener:
        'Banned: window/document `addEventListener` for custom event "{{event}}". OS events (keydown, mousemove, etc.) are allowed; feature-named events are not. See .claude/skills/s-arch/invariants.md#r2.',
    },
  },
  create(context) {
    return {
      NewExpression(node) {
        if (node.callee.type === 'Identifier' && node.callee.name === 'CustomEvent') {
          context.report({ node, messageId: 'noCustomEvent' });
        }
      },
      CallExpression(node) {
        const callee = node.callee;
        if (callee.type !== 'MemberExpression') return;
        if (callee.computed) return;
        if (callee.property.type !== 'Identifier') return;
        const propName = callee.property.name;
        if (callee.object.type !== 'Identifier') return;
        const objName = callee.object.name;
        if (!BUS_RECEIVER_NAMES.has(objName)) return;

        if (propName === 'dispatchEvent') {
          context.report({ node, messageId: 'noDispatchEvent' });
          return;
        }
        if (propName === 'addEventListener' || propName === 'removeEventListener') {
          // First arg is the event name -- allow OS event names.
          const arg = node.arguments[0];
          if (arg && arg.type === 'Literal' && typeof arg.value === 'string') {
            const eventName = arg.value;
            if (!ALLOWED_EVENT_NAMES.has(eventName)) {
              context.report({
                node,
                messageId: 'noCustomListener',
                data: { event: eventName },
              });
            }
          }
        }
      },
    };
  },
};
