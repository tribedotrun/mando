// Boundary funnels (apiRequestInternal, fromResponse, fromIpc, fromSseMessage,
// fromFile, parseWith) must receive a schema argument that resolves to an exported
// identifier from the daemon-contract or ipc-contract schemas modules. This catches
// the regression where a caller forgets to plumb a schema through.

const FUNNEL_NAMES = new Set([
  'fromResponse',
  'fromIpc',
  'fromSseMessage',
  'fromFile',
  'parseWith',
]);

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Boundary funnel calls must pass a schema identifier.' },
    messages: {
      missing:
        '{{name}}() requires a schema argument. Pass an identifier from #shared/daemon-contract/schemas or #shared/ipc-contract.',
    },
    schema: [],
  },
  create(context) {
    // shared/result/ is the implementation module; its internal calls pass `schema`
    // as a parameter (type ZodType<T>), not a *Schema identifier. Skip this folder.
    const filename = context.filename ?? context.getFilename();
    if (/\/shared\/result\//.test(filename)) return {};

    return {
      CallExpression(node) {
        const callee = node.callee;
        const name = callee.type === 'Identifier' ? callee.name : null;
        if (!name || !FUNNEL_NAMES.has(name)) return;
        // Schema is the last argument for parseWith/fromSseMessage; second arg for fromResponse/fromFile.
        // Generic check: at least one argument must be an Identifier ending in `Schema`.
        const hasSchema = node.arguments.some(
          (arg) =>
            (arg.type === 'Identifier' && /Schema$/.test(arg.name)) ||
            (arg.type === 'MemberExpression' &&
              arg.property.type === 'Identifier' &&
              /Schema$/.test(arg.property.name)),
        );
        if (!hasSchema) {
          context.report({ node, messageId: 'missing', data: { name } });
        }
      },
    };
  },
};
