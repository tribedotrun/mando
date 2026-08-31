const DAEMON_QUERY_CALLS = new Set(['toReactQuery', 'apiGetRouteR']);
const SYNC_POLICY_SUFFIX = '/renderer/global/repo/syncPolicy.ts';

function normalizeFilename(context) {
  return (context.filename || context.getFilename?.() || '').replaceAll('\\', '/');
}

function isProductionFile(filename) {
  return !filename.includes('/__tests__/') && !/\.(?:test|spec)\.tsx?$/.test(filename);
}

function unwrap(node) {
  while (
    node &&
    (node.type === 'TSAsExpression' ||
      node.type === 'TSTypeAssertion' ||
      node.type === 'TSSatisfiesExpression' ||
      node.type === 'TSNonNullExpression' ||
      node.type === 'ChainExpression')
  ) {
    node = node.expression;
  }
  return node;
}

function staticName(node) {
  const target = unwrap(node);
  if (!target) return null;
  if (target.type === 'Identifier') return target.name;
  if (target.type !== 'MemberExpression') return null;
  if (!target.computed && target.property.type === 'Identifier') return target.property.name;
  if (target.computed && target.property.type === 'Literal') return target.property.value;
  return null;
}

function objectProperty(object, name) {
  return object.properties.find(
    (property) =>
      property.type === 'Property' &&
      !property.computed &&
      staticName(property.key) === name,
  );
}

function containsNamedCall(node, names, visitorKeys) {
  const target = unwrap(node);
  if (!target || typeof target.type !== 'string') return false;
  if (target.type === 'CallExpression' && names.has(staticName(target.callee))) return true;

  for (const key of visitorKeys[target.type] ?? []) {
    const value = target[key];
    if (Array.isArray(value)) {
      if (value.some((child) => containsNamedCall(child, names, visitorKeys))) return true;
    } else if (containsNamedCall(value, names, visitorKeys)) {
      return true;
    }
  }
  return false;
}

function daemonSyncMode(metaProperty) {
  if (!metaProperty || metaProperty.type !== 'Property') return null;
  const value = unwrap(metaProperty.value);
  if (value?.type !== 'CallExpression' || staticName(value.callee) !== 'daemonSyncMeta') {
    return null;
  }
  const mode = unwrap(value.arguments[0]);
  return mode?.type === 'Literal' && typeof mode.value === 'string' ? mode.value : 'declared';
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Require explicit daemon cache sync policy on daemon-backed queries.' },
    schema: [],
    messages: {
      missingMeta: 'Daemon-backed useQuery must declare meta: daemonSyncMeta(...).',
      pollingMeta:
        "Daemon-backed useQuery with refetchInterval must declare daemonSyncMeta('polling', ...).",
      bareInvalidate:
        'Bare invalidateQueries() must go through invalidateAllDaemonQueries(reason) in syncPolicy.ts.',
    },
  },
  create(context) {
    const filename = normalizeFilename(context);
    if (!isProductionFile(filename)) return {};
    const visitorKeys = context.sourceCode.visitorKeys;

    return {
      CallExpression(node) {
        const calleeName = staticName(node.callee);

        if (calleeName === 'invalidateQueries' && node.arguments.length === 0) {
          if (!filename.endsWith(SYNC_POLICY_SUFFIX)) {
            context.report({ node, messageId: 'bareInvalidate' });
          }
          return;
        }

        if (calleeName !== 'useQuery') return;
        const options = unwrap(node.arguments[0]);
        if (options?.type !== 'ObjectExpression') return;
        if (!containsNamedCall(options, DAEMON_QUERY_CALLS, visitorKeys)) return;

        const metaProperty = objectProperty(options, 'meta');
        const mode = daemonSyncMode(metaProperty);
        if (mode === null) {
          context.report({ node: options, messageId: 'missingMeta' });
        }

        const refetchProperty = objectProperty(options, 'refetchInterval');
        if (refetchProperty && mode !== 'polling') {
          context.report({ node: refetchProperty, messageId: 'pollingMeta' });
        }
      },
    };
  },
};
