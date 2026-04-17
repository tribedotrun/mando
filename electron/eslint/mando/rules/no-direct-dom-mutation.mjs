// React owns the DOM in the renderer. Direct .textContent / .innerHTML /
// .innerText assignment fights React's reconciler.

const BANNED_PROPS = new Set(['textContent', 'innerHTML', 'innerText']);

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Ban direct DOM mutation in renderer files.' },
    messages: {
      noMutation:
        'Direct DOM mutation via .{{prop}} assignment. Let React manage DOM via props/state.',
    },
  },
  create(context) {
    return {
      AssignmentExpression(node) {
        if (node.left.type !== 'MemberExpression') return;
        const prop = node.left.property;
        let name;
        if (prop.type === 'Identifier' && !node.left.computed) name = prop.name;
        else if (prop.type === 'Literal' && typeof prop.value === 'string') name = prop.value;
        if (name && BANNED_PROPS.has(name)) {
          context.report({ node, messageId: 'noMutation', data: { prop: name } });
        }
      },
    };
  },
};
