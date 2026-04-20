// Every method named `on*` declared in preload type files must return
// `() => void` (the caller-owned unsubscribe). Catches the legacy shape
// where on* returned `void` and channel-wide cleanup was required.
//
// Codifies invariant B1/B2 in .claude/skills/s-arch/invariants.md.

const ON_METHOD_RE = /^on[A-Z]/;
const TARGET_FILE_RE = /(^|\/)preload\/types\/(api|api-channel-map)\.ts$/;

function isApplicable(filename) {
  if (!filename) return false;
  const norm = filename.replaceAll('\\', '/');
  return TARGET_FILE_RE.test(norm);
}

function isVoidReturn(annotation) {
  if (!annotation) return false;
  const ret = annotation.typeAnnotation;
  if (!ret) return false;
  if (ret.type === 'TSVoidKeyword') return true;
  return false;
}

function returnsDisposer(annotation) {
  if (!annotation) return false;
  const ret = annotation.typeAnnotation;
  if (!ret) return false;
  // Function type: () => void (strict; `() => unknown` / `() => any` are
  // not accepted — the rule docs say `() => void` only).
  if (ret.type === 'TSFunctionType' || ret.type === 'TSConstructorType') {
    const inner = ret.returnType?.typeAnnotation;
    if (!inner) return false;
    return inner.type === 'TSVoidKeyword';
  }
  return false;
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Preload `on*` methods must return `() => void` (caller-owned unsubscribe).',
    },
    messages: {
      mustReturnDisposer:
        'Preload subscribe method `{{name}}` must return `() => void` (the unsubscribe function), not `{{actual}}`. See .claude/skills/s-arch/invariants.md#b1.',
    },
  },
  create(context) {
    const filename = context.filename || (context.getFilename && context.getFilename());
    if (!isApplicable(filename)) return {};

    function checkMethodLike(node, name, returnAnnotation) {
      if (!ON_METHOD_RE.test(name)) return;
      if (returnsDisposer(returnAnnotation)) return;
      const actual = isVoidReturn(returnAnnotation) ? 'void' : 'non-disposer';
      context.report({
        node,
        messageId: 'mustReturnDisposer',
        data: { name, actual },
      });
    }

    return {
      // interface MandoAPI { onShortcut: (cb: ...) => () => void; }
      // The property is a TSPropertySignature whose typeAnnotation is a TSFunctionType.
      TSPropertySignature(node) {
        if (node.key.type !== 'Identifier') return;
        const name = node.key.name;
        if (!ON_METHOD_RE.test(name)) return;
        const fnType = node.typeAnnotation?.typeAnnotation;
        if (!fnType || fnType.type !== 'TSFunctionType') return;
        // The "return type" we care about is the function's return type.
        const innerReturn = fnType.returnType;
        checkMethodLike(node, name, innerReturn);
      },
      TSMethodSignature(node) {
        if (node.key.type !== 'Identifier') return;
        const name = node.key.name;
        if (!ON_METHOD_RE.test(name)) return;
        checkMethodLike(node, name, node.returnType);
      },
    };
  },
};
