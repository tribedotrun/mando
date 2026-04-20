/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'FormData calls to apiMultipartRouteR(...) must provide a shadowBody so outbound multipart payloads still get Zod preflight validation.',
    },
    messages: {
      requireShadowBody:
        'apiMultipartRouteR(...) with FormData must pass a shadowBody as the 4th argument. See .claude/skills/s-arch/invariants.md#r17.',
    },
  },
  create(context) {
    const sourceCode = context.sourceCode ?? context.getSourceCode?.();

    function isFormDataNewExpression(node) {
      return (
        node?.type === 'NewExpression' &&
        node.callee.type === 'Identifier' &&
        node.callee.name === 'FormData'
      );
    }

    function resolveFormDataIdentifier(node) {
      if (node?.type !== 'Identifier' || !sourceCode?.getScope) return false;
      let scope = sourceCode.getScope(node);
      while (scope) {
        const variable = scope.set?.get?.(node.name);
        const def = variable?.defs?.[0];
        if (def) {
          if (def.type !== 'Variable' || def.node.type !== 'VariableDeclarator') return false;
          return isFormDataNewExpression(def.node.init);
        }
        scope = scope.upper;
      }
      return false;
    }

    function isFormDataValue(node) {
      return isFormDataNewExpression(node) || resolveFormDataIdentifier(node);
    }

    return {
      CallExpression(node) {
        if (node.callee.type !== 'Identifier' || node.callee.name !== 'apiMultipartRouteR') return;
        if (!isFormDataValue(node.arguments[1])) return;
        if (node.arguments[3]) return;
        context.report({ node, messageId: 'requireShadowBody' });
      },
    };
  },
};
