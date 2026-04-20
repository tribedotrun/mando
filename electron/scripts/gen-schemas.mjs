// Generates electron/src/shared/daemon-contract/schemas.ts from index.ts.
// Walks the TypeScript AST of the (Rust-generated) types and emits Zod schemas.
// One Zod schema per exported type alias; references between types resolve to the
// corresponding *Schema constant. The Rust codegen invokes this script after
// regenerating index.ts.

import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import ts from 'typescript';

const __dirname = dirname(fileURLToPath(import.meta.url));
const SHARED_DIR = resolve(__dirname, '..', 'src', 'shared', 'daemon-contract');
const INDEX_PATH = resolve(SHARED_DIR, 'index.ts');
const ROUTES_PATH = resolve(SHARED_DIR, 'routes.ts');
const SCHEMAS_PATH = resolve(SHARED_DIR, 'schemas.ts');

const source = readFileSync(INDEX_PATH, 'utf8');
const sourceFile = ts.createSourceFile(INDEX_PATH, source, ts.ScriptTarget.ES2022, true);
const routesSource = readFileSync(ROUTES_PATH, 'utf8');
const routesSourceFile = ts.createSourceFile(
  ROUTES_PATH,
  routesSource,
  ts.ScriptTarget.ES2022,
  true,
);

// Collect all type names in the file so we can decide whether a Type reference
// resolves to a *Schema constant in this module or needs z.lazy.
const declaredNames = new Set();
for (const stmt of sourceFile.statements) {
  if (ts.isTypeAliasDeclaration(stmt)) declaredNames.add(stmt.name.text);
}

// Render a TypeNode to a Zod expression string. Uses lazy refs for cross-type
// references because order of declarations in the file is alphabetical and
// forward references are common.
function emit(node) {
  if (ts.isTypeLiteralNode(node)) {
    const fields = node.members.map((m) => emitMember(m)).filter(Boolean);
    return `z.object({ ${fields.join(', ')} }).strict()`;
  }
  if (ts.isUnionTypeNode(node)) return emitUnion(node);
  if (ts.isArrayTypeNode(node)) return `z.array(${emit(node.elementType)})`;
  if (ts.isParenthesizedTypeNode(node)) return emit(node.type);
  if (ts.isLiteralTypeNode(node)) return emitLiteral(node.literal);
  if (ts.isTypeReferenceNode(node)) return emitTypeRef(node);
  if (node.kind === ts.SyntaxKind.StringKeyword) return 'z.string()';
  if (node.kind === ts.SyntaxKind.NumberKeyword) return 'z.number()';
  if (node.kind === ts.SyntaxKind.BooleanKeyword) return 'z.boolean()';
  if (node.kind === ts.SyntaxKind.NullKeyword) return 'z.null()';
  if (node.kind === ts.SyntaxKind.UndefinedKeyword) return 'z.undefined()';
  if (node.kind === ts.SyntaxKind.UnknownKeyword) return 'z.unknown()';
  if (node.kind === ts.SyntaxKind.AnyKeyword) return 'z.unknown()';
  if (node.kind === ts.SyntaxKind.NeverKeyword) return 'z.never()';
  if (node.kind === ts.SyntaxKind.SymbolKeyword) return 'z.symbol()';
  if (ts.isMappedTypeNode(node)) return emitMapped(node);
  if (ts.isIntersectionTypeNode(node)) {
    const parts = node.types.map(emit);
    return parts.reduce((acc, p) => `${acc}.and(${p})`);
  }
  // Fallback for shapes the codegen doesn't yet understand. Logged as a warning.
  process.stderr.write(`gen-schemas: unhandled type node kind ${ts.SyntaxKind[node.kind]}\n`);
  return 'z.unknown()';
}

function emitMember(member) {
  if (!ts.isPropertySignature(member) || !member.type) return null;
  const name = member.name.getText(sourceFile);
  const optional = !!member.questionToken;
  const inner = emit(member.type);
  const safeName = /^[A-Za-z_$][A-Za-z0-9_$]*$/.test(name) ? name : JSON.stringify(name);
  return `${safeName}: ${optional ? `${inner}.optional()` : inner}`;
}

function emitLiteral(literal) {
  if (ts.isStringLiteral(literal)) return `z.literal(${JSON.stringify(literal.text)})`;
  if (ts.isNumericLiteral(literal)) return `z.literal(${literal.text})`;
  if (literal.kind === ts.SyntaxKind.TrueKeyword) return 'z.literal(true)';
  if (literal.kind === ts.SyntaxKind.FalseKeyword) return 'z.literal(false)';
  if (literal.kind === ts.SyntaxKind.NullKeyword) return 'z.null()';
  process.stderr.write(`gen-schemas: unhandled literal kind ${ts.SyntaxKind[literal.kind]}\n`);
  return 'z.unknown()';
}

function emitUnion(node) {
  const types = node.types;
  // Detect "T | null" -> nullable
  const nullIdx = types.findIndex(
    (t) =>
      (ts.isLiteralTypeNode(t) && t.literal.kind === ts.SyntaxKind.NullKeyword) ||
      t.kind === ts.SyntaxKind.NullKeyword,
  );
  if (nullIdx >= 0 && types.length === 2) {
    const other = types[nullIdx === 0 ? 1 : 0];
    return `${emit(other)}.nullable()`;
  }
  // Pure string-literal enum -> z.enum
  if (types.every((t) => ts.isLiteralTypeNode(t) && ts.isStringLiteral(t.literal))) {
    const values = types.map((t) => JSON.stringify(t.literal.text));
    return `z.enum([${values.join(', ')}])`;
  }
  // Plain union; handle null separately by partitioning
  const nullPresent = types.some(
    (t) =>
      (ts.isLiteralTypeNode(t) && t.literal.kind === ts.SyntaxKind.NullKeyword) ||
      t.kind === ts.SyntaxKind.NullKeyword,
  );
  const nonNull = types.filter(
    (t) =>
      !(ts.isLiteralTypeNode(t) && t.literal.kind === ts.SyntaxKind.NullKeyword) &&
      t.kind !== ts.SyntaxKind.NullKeyword,
  );
  if (nonNull.length === 1 && nullPresent) return `${emit(nonNull[0])}.nullable()`;
  const variants = nonNull.map(emit);
  const base = `z.union([${variants.join(', ')}])`;
  return nullPresent ? `${base}.nullable()` : base;
}

function emitTypeRef(node) {
  const name = node.typeName.getText(sourceFile);
  if (name === 'Array' && node.typeArguments?.length === 1) {
    return `z.array(${emit(node.typeArguments[0])})`;
  }
  if (name === 'Record' && node.typeArguments?.length === 2) {
    return `z.record(${emit(node.typeArguments[0])}, ${emit(node.typeArguments[1])})`;
  }
  if (declaredNames.has(name)) {
    // lazy ref to support forward / circular references
    return `z.lazy(() => ${schemaName(name)})`;
  }
  process.stderr.write(`gen-schemas: unknown type ref "${name}", emitting z.unknown()\n`);
  return 'z.unknown()';
}

// Mapped type: { [key in K]: V } -> z.record(z.string(), V) when K is string
function emitMapped(node) {
  const valueType = node.type ? emit(node.type) : 'z.unknown()';
  return `z.record(z.string(), ${valueType})`;
}

function schemaName(typeName) {
  return typeName.charAt(0).toLowerCase() + typeName.slice(1) + 'Schema';
}

// Walk the source file, emit one schema per type alias declaration.
const out = [];
out.push('// Auto-generated by scripts/gen-schemas.mjs. Do not edit by hand.');
out.push('// Source: index.ts + routes.ts (themselves generated by api-types-codegen).');
out.push("import { z } from 'zod';");
out.push('');
for (const stmt of sourceFile.statements) {
  if (!ts.isTypeAliasDeclaration(stmt)) continue;
  const name = stmt.name.text;
  const expr = emit(stmt.type);
  out.push(`export const ${schemaName(name)} = ${expr};`);
}
out.push('');

// Emit per-route schema lookup maps. Funnels resolve schemas from the route key
// without callers having to import them.
const routeMaps = collectRouteSchemas(routesSourceFile);
emitRouteMap(out, 'resSchemas', routeMaps.res);
emitRouteMap(out, 'eventSchemas', routeMaps.event);
emitRouteMap(out, 'bodySchemas', routeMaps.body);
emitRouteMap(out, 'querySchemas', routeMaps.query);
emitRouteMap(out, 'paramsSchemas', routeMaps.params);
out.push('');

function collectRouteSchemas(routesSf) {
  const res = {};
  const event = {};
  const body = {};
  const query = {};
  const params = {};
  function walk(node) {
    if (ts.isInterfaceDeclaration(node) && node.name.text === 'Routes') {
      for (const member of node.members) {
        if (!ts.isPropertySignature(member) || !member.type) continue;
        const routeKey = member.name.getText(routesSf);
        if (!ts.isTypeLiteralNode(member.type)) continue;
        for (const inner of member.type.members) {
          if (!ts.isPropertySignature(inner) || !inner.type) continue;
          const propName = inner.name.getText(routesSf);
          const schemaExpr = emitRouteSchema(inner.type, routesSf);
          if (!schemaExpr) continue;
          if (propName === 'res') res[routeKey] = schemaExpr;
          else if (propName === 'event') event[routeKey] = schemaExpr;
          else if (propName === 'body') body[routeKey] = schemaExpr;
          else if (propName === 'query') query[routeKey] = schemaExpr;
          else if (propName === 'params') params[routeKey] = schemaExpr;
        }
      }
    }
    ts.forEachChild(node, walk);
  }
  walk(routesSf);
  return { res, event, body, query, params };
}

function emitRouteSchema(typeNode, sf) {
  if (ts.isParenthesizedTypeNode(typeNode)) return emitRouteSchema(typeNode.type, sf);
  if (ts.isArrayTypeNode(typeNode)) {
    const element = emitRouteSchema(typeNode.elementType, sf);
    return element ? `z.array(${element})` : null;
  }
  if (ts.isTypeReferenceNode(typeNode)) {
    const rawName = typeNode.typeName.getText(sf).replace(/^Types\./, '');
    if (rawName === 'Array' && typeNode.typeArguments?.length === 1) {
      const element = emitRouteSchema(typeNode.typeArguments[0], sf);
      return element ? `z.array(${element})` : null;
    }
    if (declaredNames.has(rawName)) return schemaName(rawName);
  }
  return null;
}

function emitRouteMap(buf, varName, map) {
  const entries = Object.entries(map).sort(([a], [b]) => a.localeCompare(b));
  if (entries.length === 0) {
    buf.push(`export const ${varName}: Record<string, never> = {};`);
    return;
  }
  buf.push(`export const ${varName} = {`);
  for (const [routeKey, schema] of entries) {
    buf.push(`  ${routeKey}: ${schema},`);
  }
  buf.push('} as const;');
}

// CLI mode: --stdout prints to stdout (used by Rust codegen so it controls writes);
// no flag writes to schemas.ts directly (used by `npm run gen:schemas`).
const content = out.join('\n');
if (process.argv.includes('--stdout')) {
  process.stdout.write(content);
} else {
  writeFileSync(SCHEMAS_PATH, content, 'utf8');
  process.stderr.write(`gen-schemas: wrote ${SCHEMAS_PATH}\n`);
}
