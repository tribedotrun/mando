// Each `.tsx` file under src/renderer/ must have a named or default export
// whose identifier matches the filename stem. When a file exports multiple
// unrelated top-level components, it should be split into a subfolder barrel
// so each leaf file still matches its primary export.
//
// Scope: `.tsx` files under `/src/renderer/`. Utility `.ts` files and
// declaration `.d.ts` files are out of scope (see `isRendererTsx` below).
//
// Exemptions within scope:
//   - index.tsx (barrel files are stem-neutral by purpose)
//   - Files inside src/renderer/global/ui/primitives/ (shadcn-style kebab-case
//     primitive files predate this rule and follow a different convention)
//   - Test files under __tests__/ or matching `*.test.tsx`
import path from 'node:path';

function getStem(filename) {
  const base = path.basename(filename);
  return base.replace(/\.tsx$/, '');
}

function isExempt(filename) {
  const norm = filename.replaceAll('\\', '/');
  if (/\/index\.tsx$/.test(norm)) return true;
  if (norm.includes('/renderer/global/ui/primitives/')) return true;
  if (norm.includes('/__tests__/') || norm.endsWith('.test.tsx')) return true;
  return false;
}

// Detector 1 scope is component files (.tsx) only. Utility/.ts files and
// declaration (.d.ts) files are out of scope — they are expected to be
// grab-bag modules (e.g. utils.ts, service helpers) and forcing a
// single-export convention there would balloon the invariant.
function isRendererTsx(filename) {
  const norm = filename.replaceAll('\\', '/');
  return norm.includes('/src/renderer/') && norm.endsWith('.tsx');
}

function collectExportNames(node) {
  const names = [];
  if (node.type !== 'ExportNamedDeclaration') return names;
  if (node.declaration) {
    const decl = node.declaration;
    if (decl.type === 'FunctionDeclaration' || decl.type === 'ClassDeclaration') {
      if (decl.id) names.push(decl.id.name);
    } else if (decl.type === 'VariableDeclaration') {
      for (const d of decl.declarations) {
        if (d.id && d.id.type === 'Identifier') names.push(d.id.name);
      }
    } else if (decl.type === 'TSTypeAliasDeclaration' || decl.type === 'TSInterfaceDeclaration' || decl.type === 'TSEnumDeclaration') {
      if (decl.id) names.push(decl.id.name);
    }
  }
  if (node.specifiers) {
    for (const spec of node.specifiers) {
      if (spec.exported && spec.exported.type === 'Identifier') {
        names.push(spec.exported.name);
      }
    }
  }
  return names;
}

function collectDefaultExportName(node) {
  if (node.type !== 'ExportDefaultDeclaration') return null;
  const d = node.declaration;
  if (!d) return null;
  if ((d.type === 'FunctionDeclaration' || d.type === 'ClassDeclaration') && d.id) {
    return d.id.name;
  }
  if (d.type === 'Identifier') return d.name;
  return null;
}

function hasAnonymousDefaultExport(programNode) {
  for (const child of programNode.body) {
    if (child.type !== 'ExportDefaultDeclaration') continue;
    const d = child.declaration;
    if (!d) continue;
    if (d.type === 'ArrowFunctionExpression') return true;
    if (d.type === 'FunctionExpression' && !d.id) return true;
    if (d.type === 'ClassExpression' && !d.id) return true;
    if (d.type === 'FunctionDeclaration' && !d.id) return true;
    if (d.type === 'ClassDeclaration' && !d.id) return true;
  }
  return false;
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description:
        'Filename stem must match a top-level named or default export. Grab-bag files should be split into a subfolder barrel.',
    },
    messages: {
      stemMismatch:
        'Filename stem "{{stem}}" does not match any top-level export (found: [{{exports}}]). Rename the file to its primary export, rename an export to the stem, or split into a subfolder barrel so each leaf file matches its export.',
      noExports:
        'File has no top-level exports but must expose an identifier named "{{stem}}" to match its filename.',
      anonDefault:
        'Default export has no identifier. Use `export default function {{stem}}()` or add a named export "{{stem}}" so the file matches its filename.',
    },
  },
  create(context) {
    const filename = context.filename || context.getFilename();
    if (!filename) return {};
    if (!isRendererTsx(filename)) return {};
    if (isExempt(filename)) return {};
    const stem = getStem(filename);

    return {
      Program(programNode) {
        const exports = new Set();
        for (const child of programNode.body) {
          for (const n of collectExportNames(child)) exports.add(n);
          const defName = collectDefaultExportName(child);
          if (defName) exports.add(defName);
        }

        if (exports.size === 0) {
          if (hasAnonymousDefaultExport(programNode)) {
            context.report({
              node: programNode,
              messageId: 'anonDefault',
              data: { stem },
            });
            return;
          }
          context.report({
            node: programNode,
            messageId: 'noExports',
            data: { stem },
          });
          return;
        }

        if (!exports.has(stem)) {
          context.report({
            node: programNode,
            messageId: 'stemMismatch',
            data: { stem, exports: [...exports].join(', ') },
          });
        }
      },
    };
  },
};
