// PR #883 invariant #2: no silent error absorption.
//
// Rejects unreferenced error parameters in five specific callback shapes
// where swallowing the error has bitten us historically:
//
//   - `try { ... } catch (err) { ... }`
//   - `promise.catch((err) => { ... })`
//   - `useMutation({ onError: (err) => { ... } })`
//   - `useMutation({ onSuccess: (data, variables, context) => { ... } })`
//     — the positional params beyond the first are allowed to be unused;
//     this rule only scopes to `onError`-like shapes.
//   - `useMutation({ onSettled: (data, error) => { ... } })`
//     — the `error` argument (index 1) must be referenced.
//
// Outside these shapes the project's standard "_-prefix signals
// intentionally-unused" rule stays in force. The rationale: in a caught
// error / rejection / mutation-error callback, the error value is the
// whole reason the callback fired. Dropping it silently is a build
// failure; the fix is either a reference (route it into the logger or
// surface it to the user) or an explicit `_err` prefix IF the call site
// has a separate structured log covering the same code path.
//
// Because the "separate structured log" escape hatch is subjective, the
// rule requires the callback body to either (a) reference the error
// identifier at least once, or (b) invoke the structured logger (by
// syntactic-name match) within the same callback.

const LOGGER_NAMES = new Set([
  'getLogger',
  'logger',
  'log',
  'rendererLogger',
  'mainLogger',
  'preloadLogger',
]);

function hasIdentifierReference(node, name) {
  if (!node) return false;
  let found = false;
  const visit = (n) => {
    if (found || !n || typeof n !== 'object') return;
    if (Array.isArray(n)) {
      for (const child of n) visit(child);
      return;
    }
    if (n.type === 'Identifier' && n.name === name) {
      found = true;
      return;
    }
    for (const key of Object.keys(n)) {
      if (key === 'parent') continue;
      visit(n[key]);
    }
  };
  visit(node);
  return found;
}

function hasLoggerCall(node) {
  if (!node) return false;
  let found = false;
  const visit = (n) => {
    if (found || !n || typeof n !== 'object') return;
    if (Array.isArray(n)) {
      for (const child of n) visit(child);
      return;
    }
    if (n.type === 'CallExpression') {
      const callee = n.callee;
      // Pattern 1: bare logger call like `logger('msg')` or `log('msg')`.
      if (callee?.type === 'Identifier' && LOGGER_NAMES.has(callee.name)) found = true;
      if (callee?.type === 'MemberExpression' && callee.property?.type === 'Identifier') {
        // Pattern 2: `something.log(...)` / `something.getLogger()` — property is a logger name.
        if (LOGGER_NAMES.has(callee.property.name)) found = true;
        // Pattern 3: `log.error(...)` / `logger.warn(...)` — object is a logger name.
        if (callee.object?.type === 'Identifier' && LOGGER_NAMES.has(callee.object.name)) {
          found = true;
        }
      }
    }
    if (found) return;
    for (const key of Object.keys(n)) {
      if (key === 'parent') continue;
      visit(n[key]);
    }
  };
  visit(node);
  return found;
}

// Walk an ObjectPattern / ArrayPattern / AssignmentPattern and collect
// every bound Identifier name — so destructured error params like
// `catch ({ message, code })` or `onError: ([err]) => ...` still count
// as "did the callback reference any piece of the error?"
function collectBoundNames(pattern, names) {
  if (!pattern || typeof pattern !== 'object') return;
  if (pattern.type === 'Identifier') {
    names.push(pattern.name);
    return;
  }
  if (pattern.type === 'ObjectPattern') {
    for (const prop of pattern.properties ?? []) {
      if (prop.type === 'Property') collectBoundNames(prop.value, names);
      else if (prop.type === 'RestElement') collectBoundNames(prop.argument, names);
    }
    return;
  }
  if (pattern.type === 'ArrayPattern') {
    for (const el of pattern.elements ?? []) collectBoundNames(el, names);
    return;
  }
  if (pattern.type === 'AssignmentPattern') {
    collectBoundNames(pattern.left, names);
    return;
  }
  if (pattern.type === 'RestElement') {
    collectBoundNames(pattern.argument, names);
    return;
  }
  // TS-specific: `constructor(private err: T)` uses TSParameterProperty.
  // Not currently reachable in catch/arrow positions, but covered so the
  // claim "every pattern type is handled" stays true and any future AST
  // shape change doesn't silently bypass the rule.
  if (pattern.type === 'TSParameterProperty') {
    collectBoundNames(pattern.parameter, names);
  }
}

function reportIfUnused(context, callbackNode, errorParam, label) {
  if (!errorParam) return;
  const body = callbackNode.body;
  // Underscore-prefix is a hard signal elsewhere in the project that
  // the author wanted to opt out. This rule deliberately overrides
  // that convention for the scoped shapes above — "_err" in a catch
  // block is exactly the pattern that bit us and that PR #883 names.
  if (errorParam.type === 'Identifier') {
    const name = errorParam.name;
    if (hasIdentifierReference(body, name)) return;
    if (hasLoggerCall(body)) return;
    context.report({ node: errorParam, messageId: 'unused', data: { label, name } });
    return;
  }
  // Destructuring shapes — collect every bound name and require at
  // least one to be referenced (or a logger call in the body). Without
  // this, `catch ({ message }) { toast('failed'); }` would bypass the
  // rule entirely.
  const bound = [];
  collectBoundNames(errorParam, bound);
  if (bound.some((n) => hasIdentifierReference(body, n))) return;
  if (hasLoggerCall(body)) return;
  context.report({
    node: errorParam,
    messageId: 'unusedDestructured',
    data: { label },
  });
}

function isCatchCallback(node) {
  const parent = node.parent;
  return (
    parent?.type === 'CallExpression' &&
    parent.arguments[0] === node &&
    parent.callee?.type === 'MemberExpression' &&
    parent.callee.property?.type === 'Identifier' &&
    parent.callee.property.name === 'catch'
  );
}

function isScopedMutationProperty(node, names) {
  // node is ArrowFunctionExpression / FunctionExpression; check that it
  // is the value of a Property whose key is in `names`.
  const parent = node.parent;
  if (parent?.type !== 'Property') return null;
  const key = parent.key;
  if (!key) return null;
  const keyName = key.type === 'Identifier' ? key.name : key.type === 'Literal' ? key.value : null;
  if (!names.has(keyName)) return null;
  return keyName;
}

const MUTATION_ERROR_KEYS = new Set(['onError']);
const MUTATION_SETTLED_KEYS = new Set(['onSettled']);
// onSuccess receives (data, variables, context) — no error parameter.

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Reject unused error parameters in catch / Promise.catch / useMutation onError / onSettled callbacks.',
    },
    messages: {
      unused:
        'Error parameter `{{name}}` in {{label}} is not referenced. Log it or handle it — silent discard is a PR #883 invariant violation.',
      unusedDestructured:
        'Destructured error parameter in {{label}} has no referenced field and no logger call. Log it or handle at least one field — silent discard is a PR #883 invariant violation.',
    },
    schema: [],
  },
  create(context) {
    function handleCallbackLike(node) {
      if (isCatchCallback(node)) {
        reportIfUnused(context, node, node.params[0], 'Promise.catch');
        return;
      }
      const mutationKey = isScopedMutationProperty(node, MUTATION_ERROR_KEYS);
      if (mutationKey) {
        reportIfUnused(context, node, node.params[0], `useMutation ${mutationKey}`);
        return;
      }
      const settledKey = isScopedMutationProperty(node, MUTATION_SETTLED_KEYS);
      if (settledKey) {
        // onSettled: (data, error, variables, context) — error is index 1.
        reportIfUnused(context, node, node.params[1], `useMutation ${settledKey}`);
      }
    }

    return {
      CatchClause(node) {
        if (!node.param) return;
        // Allow both `catch (err)` and `catch ({ field })` / `catch ([err])`
        // — reportIfUnused handles destructuring via collectBoundNames.
        reportIfUnused(
          context,
          { body: node.body },
          node.param,
          'try/catch',
        );
      },
      ArrowFunctionExpression: handleCallbackLike,
      FunctionExpression: handleCallbackLike,
    };
  },
};
