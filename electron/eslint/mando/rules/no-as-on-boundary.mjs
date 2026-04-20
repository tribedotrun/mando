// Bans `as T` on raw boundary input. Boundary data must be parsed (Zod) before use.
// Triggered when `as` is applied to:
//   - JSON.parse(...)
//   - <Response>.json()
//   - .data on a MessageEvent / SSE event payload
//   - ipcRenderer.invoke(...)
//   - await fetch(...)
//
// Allowed: `as const`, `as unknown as T` paired with adjacent `// reason:` comment.

const MSG = 'Cast `as {{kind}}` on boundary data is banned. Parse via Zod schema first.';

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: { description: 'Ban as-cast on raw boundary data; require schema parse.' },
    messages: { boundary: MSG },
    schema: [],
  },
  create(context) {
    function describe(node) {
      // CallExpression
      if (node.type === 'CallExpression') {
        const callee = node.callee;
        if (callee.type === 'MemberExpression') {
          const object =
            callee.object.type === 'Identifier' ? callee.object.name : null;
          const prop =
            callee.property.type === 'Identifier' ? callee.property.name : null;
          if (object === 'JSON' && prop === 'parse') return 'JSON.parse';
          if (prop === 'json') return '.json()';
          if (object === 'ipcRenderer' && prop === 'invoke') return 'ipcRenderer.invoke';
        }
        if (callee.type === 'Identifier' && callee.name === 'fetch') return 'fetch()';
      }
      // AwaitExpression unwraps inner CallExpression
      if (node.type === 'AwaitExpression') return describe(node.argument);
      // MessageEvent / SSE: msg.data, e.data, event.data, also chained event.data.data
      if (node.type === 'MemberExpression' && node.property.type === 'Identifier' && node.property.name === 'data') {
        const objectName = node.object.type === 'Identifier' ? node.object.name : null;
        if (objectName && /^(msg|e|ev|event|message|payload)$/.test(objectName)) {
          return `${objectName}.data`;
        }
        // chained: event.data.data (SSE envelope -> inner payload)
        if (node.object.type === 'MemberExpression' && node.object.property.type === 'Identifier' && node.object.property.name === 'data') {
          const outer = node.object.object.type === 'Identifier' ? node.object.object.name : null;
          if (outer && /^(msg|e|ev|event|message|payload)$/.test(outer)) {
            return `${outer}.data.data`;
          }
        }
      }
      return null;
    }

    function isAsConst(node) {
      // `as const` parses as TSTypeReference where typeName.name === 'const'
      const t = node.typeAnnotation;
      return t && t.type === 'TSTypeReference' && t.typeName && t.typeName.name === 'const';
    }

    // Narrowing casts that don't assert an API shape: unknown, Record<string, unknown>,
    // Record<string, T> with primitive T. These pair with runtime typeof guards.
    function isPassthroughNarrowing(node) {
      const t = node.typeAnnotation;
      if (!t) return false;
      if (t.type === 'TSUnknownKeyword') return true;
      // Union: every variant must be passthrough
      if (t.type === 'TSUnionType') {
        return t.types.every((v) =>
          v.type === 'TSUnknownKeyword' ||
          v.type === 'TSNullKeyword' ||
          (v.type === 'TSLiteralType' && v.literal && v.literal.type === 'TSNullKeyword') ||
          (v.type === 'TSUndefinedKeyword') ||
          isRecordOfUnknown(v),
        );
      }
      return isRecordOfUnknown(t);
    }

    function isRecordOfUnknown(t) {
      if (t.type !== 'TSTypeReference') return false;
      const name = t.typeName.type === 'Identifier' ? t.typeName.name : null;
      if (name !== 'Record') return false;
      // ts-eslint AST: type args live under `typeArguments.params`
      const params = t.typeArguments?.params ?? t.typeParameters?.params ?? [];
      if (params.length !== 2) return false;
      const k = params[0];
      const v = params[1];
      return (
        k.type === 'TSStringKeyword' &&
        (v.type === 'TSUnknownKeyword' || v.type === 'TSAnyKeyword')
      );
    }

    function isDoubleUnknownCast(node) {
      // The narrow escape hatch: `expr as unknown as T`. The outer TSAsExpression
      // (the one reaching this rule) must have its argument be another TSAsExpression
      // that casts to `unknown`. Any other shape is not the approved double-cast form.
      return (
        node.expression.type === 'TSAsExpression' &&
        node.expression.typeAnnotation.type === 'TSUnknownKeyword'
      );
    }

    function hasReasonComment(node) {
      // `// reason:` only unlocks the narrow `as unknown as T` double-cast form.
      // A plain `await res.json() as Foo` with a reason comment is still banned.
      if (!isDoubleUnknownCast(node)) return false;
      // Walk up to the nearest "comment-attaching" ancestor: a statement OR an
      // object-literal property. Comments commonly sit above the property line.
      let cursor = node;
      while (cursor && cursor.parent) {
        cursor = cursor.parent;
        if (
          cursor.type === 'VariableDeclaration' ||
          cursor.type === 'ExpressionStatement' ||
          cursor.type === 'ReturnStatement' ||
          cursor.type === 'IfStatement' ||
          cursor.type === 'ThrowStatement' ||
          cursor.type === 'Property' ||
          cursor.type === 'PropertyDefinition' ||
          cursor.type === 'MethodDefinition'
        )
          break;
      }
      if (!cursor) return false;
      const before = context.sourceCode.getCommentsBefore(cursor);
      const inline = context.sourceCode.getCommentsAfter(node);
      return [...before, ...inline].some((c) => /reason:/i.test(c.value));
    }

    return {
      TSAsExpression(node) {
        if (isAsConst(node)) return;
        if (isPassthroughNarrowing(node)) return;
        if (hasReasonComment(node)) return;
        const inner = node.expression;
        const kind = describe(inner);
        if (!kind) return;
        context.report({
          node,
          messageId: 'boundary',
          data: { kind },
        });
      },
    };
  },
};
