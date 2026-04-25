// Functions in repo/, service/, runtime/, providers/, and global/repo|runtime|providers/
// folders that return a Promise must return a `ResultAsync` (or `Promise<Result>`) — not
// a bare Promise of T. Forces the in-house Result module everywhere a function can fail.
//
// AST-only check (no type info): looks at explicit return-type annotations and at returned
// expressions. Functions without an annotation OR returning ResultAsync/Result/toReactQuery
// are considered compliant. Functions returning a bare Promise<T> trigger the rule.

// Scope: repo/, service/, runtime/, providers/. The repo/ tier is where boundary calls
// (HTTP, IPC, file I/O) sit. Service/runtime/providers contain orchestration helpers that
// can also fail; enforcing Result there closes the remaining gaps in Pillar 5 coverage.
const TIER_RE = /\/(repo|service|runtime|providers)\//;
const UI_RE = /\/ui\/|\.tsx$/;
const TESTS_RE = /\/__tests__\/|\.test\./;
// These specific files are funnel/translator implementations; they intentionally
// produce Promise<T> as part of the boundary contract.
const ALLOWED = [
  /\/renderer\/global\/providers\/http\.ts$/,
  /\/renderer\/global\/providers\/httpRoutes\.ts$/,
  /\/main\/global\/runtime\/lifecycle\.ts$/,
  /\/main\/global\/runtime\/launchd\.ts$/,
  /\/main\/global\/runtime\/portCheck\.ts$/,
  /\/main\/global\/runtime\/uiLifecycle\.ts$/,
  /\/main\/global\/runtime\/devGitInfo\.ts$/,
  /\/main\/global\/runtime\/icons\.ts$/,
  /\/main\/global\/runtime\/appPackage\.ts$/,
  /\/main\/global\/runtime\/ipcSecurity\.ts$/,
  /\/main\/global\/runtime\/rendererServerOwner\.ts$/,
  /\/main\/global\/providers\/logger\.ts$/,
  /\/preload\/providers\/ipc\.ts$/,
  /\/shared\/result\//,
  /\/shared\/ipc-contract\//,
];

// React Query hook names whose object-literal argument properties queryFn/mutationFn
// accept a bare Promise<T>-returning function as part of the library contract.
// The toReactQuery() translator converts Result→throw at exactly this boundary;
// the function wrapping it must still return Promise<T> for the library.
const REACT_QUERY_HOOKS = new Set([
  'useQuery',
  'useMutation',
  'useInfiniteQuery',
  'useSuspenseQuery',
  'useSuspenseInfiniteQuery',
  'useQueries',
]);
const REACT_QUERY_PROP_NAMES = new Set(['queryFn', 'mutationFn', 'queryFn:']);

/**
 * Returns true when `node` is a function that is the value of a `queryFn` or
 * `mutationFn` property inside a `useQuery({...})` / `useMutation({...})` call.
 *
 * Walks: Property → ObjectExpression → CallExpression whose callee is a
 * React Query hook identifier (or member expression ending in one).
 */
function isReactQueryCallback(node) {
  const parent = node.parent;
  if (!parent) return false;

  // The function must be the value of a Property node.
  if (parent.type !== 'Property') return false;
  const propKey = parent.key;
  const propName =
    propKey.type === 'Identifier'
      ? propKey.name
      : propKey.type === 'Literal'
        ? String(propKey.value)
        : null;
  if (!REACT_QUERY_PROP_NAMES.has(propName)) return false;

  // The property must live inside an ObjectExpression.
  const obj = parent.parent;
  if (!obj || obj.type !== 'ObjectExpression') return false;

  // That ObjectExpression must be an argument to a CallExpression.
  const call = obj.parent;
  if (!call || call.type !== 'CallExpression') return false;
  if (!call.arguments.includes(obj)) return false;

  // The callee must be (or end with) a React Query hook name.
  const callee = call.callee;
  const calleeName =
    callee.type === 'Identifier'
      ? callee.name
      : callee.type === 'MemberExpression' && callee.property.type === 'Identifier'
        ? callee.property.name
        : null;
  return REACT_QUERY_HOOKS.has(calleeName);
}

function isPromiseTypeAnnotation(node) {
  if (!node) return false;
  if (node.type === 'TSTypeReference' && node.typeName.type === 'Identifier') {
    return node.typeName.name === 'Promise';
  }
  if (node.type === 'TSTypeAnnotation') return isPromiseTypeAnnotation(node.typeAnnotation);
  return false;
}

function isResultLike(node) {
  if (!node) return false;
  if (node.type === 'TSTypeReference' && node.typeName.type === 'Identifier') {
    return /^(Result|ResultAsync|Ok|Err)$/.test(node.typeName.name);
  }
  if (node.type === 'TSTypeAnnotation') return isResultLike(node.typeAnnotation);
  // Promise<Result<...>> is fine
  if (
    node.type === 'TSTypeReference' &&
    node.typeName.type === 'Identifier' &&
    node.typeName.name === 'Promise'
  ) {
    const inner = node.typeArguments?.params?.[0];
    return inner ? isResultLike(inner) : false;
  }
  return false;
}

/**
 * Returns true when any leading comment on `node` or an enclosing statement
 * node contains `invariant:`. Walks up the parent chain to the nearest
 * statement-level ancestor (VariableDeclaration, ExportNamedDeclaration,
 * ExpressionStatement, etc.) so that all of these placements work:
 *
 *   // invariant: ...
 *   export async function foo() { ... }
 *
 *   // invariant: ...
 *   const foo = async () => { ... }
 *
 *   // invariant: ...
 *   const foo = useCallback(async () => { ... }, []);
 */
function hasInvariantComment(node, context) {
  const hasComment = (n) =>
    context.sourceCode.getCommentsBefore(n).some((c) => /invariant:/i.test(c.value));

  // Walk up the parent chain until we hit a statement-level node or run out.
  let cursor = node;
  while (cursor) {
    if (hasComment(cursor)) return true;
    const p = cursor.parent;
    if (!p) break;
    // Stop at statement-level nodes after checking them.
    const isStatement =
      p.type === 'ExpressionStatement' ||
      p.type === 'VariableDeclaration' ||
      p.type === 'ExportNamedDeclaration' ||
      p.type === 'ExportDefaultDeclaration' ||
      p.type === 'ReturnStatement';
    if (isStatement) {
      return hasComment(p);
    }
    cursor = p;
  }
  return false;
}

/** Extract the tier name from the file path for use in the error message. */
function tierName(filename) {
  const m = filename.match(/\/(repo|service|runtime|providers)\//);
  return m ? m[1] : 'tier';
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Fallible non-UI functions must return Result/ResultAsync, not bare Promise<T>.',
    },
    messages: {
      bare:
        'Function in {{tier}} returns Promise<T>. Wrap in ResultAsync, or annotate return type as Promise<Result<T, E>>.',
    },
    schema: [],
  },
  create(context) {
    const filename = context.filename ?? context.getFilename();
    if (!TIER_RE.test(filename)) return {};
    if (UI_RE.test(filename)) return {};
    if (TESTS_RE.test(filename)) return {};
    if (ALLOWED.some((re) => re.test(filename))) return {};

    const tier = tierName(filename);

    function check(node) {
      if (!node.async) {
        // Only async functions implicitly return Promise<T>; sync funcs don't trigger.
        if (!node.returnType) return;
        if (!isPromiseTypeAnnotation(node.returnType.typeAnnotation)) return;
      }
      if (node.returnType && isResultLike(node.returnType.typeAnnotation)) return;
      // Promise<void> is fine — no data to parse, just a side-effect signal.
      if (node.returnType) {
        const inner = node.returnType.typeAnnotation.typeArguments?.params?.[0];
        if (inner?.type === 'TSVoidKeyword') return;
      }
      // No explicit return type AND async: TS infers Promise<T>; flag unless the body
      // returns a ResultAsync identifier directly. We'd need type info to check the body
      // properly, so we fall back to a conservative approach: only flag when the function
      // has an explicit Promise<T> annotation that is not Promise<Result<...>>.
      if (!node.returnType) return;
      if (!isPromiseTypeAnnotation(node.returnType.typeAnnotation)) return;
      const inner = node.returnType.typeAnnotation.typeArguments?.params?.[0];
      if (inner && isResultLike(inner)) return;
      // React Query queryFn/mutationFn callbacks must return bare Promise<T>; the
      // toReactQuery() translator sits at that boundary and converts Result→throw.
      if (isReactQueryCallback(node)) return;
      // Functions annotated with `// invariant:` are programmer-bug guards or
      // documented library-interop shapes — same escape hatch as no-bare-throw.
      if (hasInvariantComment(node, context)) return;
      context.report({ node, messageId: 'bare', data: { tier } });
    }

    return {
      FunctionDeclaration: check,
      FunctionExpression: check,
      ArrowFunctionExpression: check,
    };
  },
};
