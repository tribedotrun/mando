/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Ban direct DOM mutations in renderer files. React must own the DOM.' },
    messages: {
      noMutation:
        'Direct DOM mutation via .{{prop}} assignment. Let React manage DOM state through props/state.',
    },
  },
  create(context) {
    const BANNED_PROPS = new Set(['textContent', 'innerHTML', 'innerText']);

    return {
      AssignmentExpression(node) {
        const left = node.left;
        if (left.type !== 'MemberExpression') return;
        const prop = left.property;

        let propName;
        if (prop.type === 'Identifier' && !left.computed) {
          propName = prop.name;
        } else if (prop.type === 'Literal' && typeof prop.value === 'string') {
          propName = prop.value;
        }
        if (!propName) return;

        if (BANNED_PROPS.has(propName)) {
          context.report({ node, messageId: 'noMutation', data: { prop: propName } });
        }
      },
    };
  },
};
