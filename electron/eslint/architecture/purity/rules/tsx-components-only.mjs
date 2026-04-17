import { isTsxFile } from '../../../shared/constants.mjs';

function isPascalCase(name) {
  return /^[A-Z][A-Za-z0-9]*$/.test(name);
}

function initIsFunction(init) {
  if (!init) return false;
  if (init.type === 'ArrowFunctionExpression') return true;
  if (init.type === 'FunctionExpression') return true;
  if (init.type === 'CallExpression') {
    for (const arg of init.arguments) {
      if (arg.type === 'ArrowFunctionExpression' || arg.type === 'FunctionExpression') {
        return true;
      }
    }
  }
  return false;
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        '.tsx files may only declare components at top level. Non-component functions and hooks belong in a .ts sibling (runtime/ for hooks, service/ for pure).',
    },
    messages: {
      notComponent:
        '"{{name}}" is not a component. .tsx files may only declare components at top level. Move it to a .ts sibling (runtime/ if it uses React, service/ if pure). See s-arch skill.',
    },
  },
  create(context) {
    if (!isTsxFile(context.filename || context.getFilename())) return {};

    function checkStmt(stmt) {
      if (!stmt) return;
      if (
        stmt.type === 'ExportNamedDeclaration' ||
        stmt.type === 'ExportDefaultDeclaration'
      ) {
        if (stmt.declaration) checkStmt(stmt.declaration);
        return;
      }
      if (stmt.type === 'FunctionDeclaration' && stmt.id) {
        if (!isPascalCase(stmt.id.name)) {
          context.report({
            node: stmt.id,
            messageId: 'notComponent',
            data: { name: stmt.id.name },
          });
        }
        return;
      }
      if (stmt.type === 'VariableDeclaration') {
        for (const decl of stmt.declarations) {
          if (decl.id.type !== 'Identifier') continue;
          if (!initIsFunction(decl.init)) continue;
          if (!isPascalCase(decl.id.name)) {
            context.report({
              node: decl.id,
              messageId: 'notComponent',
              data: { name: decl.id.name },
            });
          }
        }
      }
    }

    return {
      Program(node) {
        for (const stmt of node.body) checkStmt(stmt);
      },
    };
  },
};
