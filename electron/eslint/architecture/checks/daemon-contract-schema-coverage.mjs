#!/usr/bin/env node
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import ts from 'typescript';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const CONTRACT_DIR = path.resolve(__dirname, '../../../src/shared/daemon-contract');
const ROUTES_FILE = path.join(CONTRACT_DIR, 'routes.ts');
const SCHEMAS_FILE = path.join(CONTRACT_DIR, 'schemas.ts');

const FIELD_TO_MAP = {
  res: 'resSchemas',
  event: 'eventSchemas',
  body: 'bodySchemas',
  query: 'querySchemas',
  params: 'paramsSchemas',
};

function parseRouteFields(content) {
  const routeFields = new Map(
    Object.keys(FIELD_TO_MAP).map((field) => [field, new Set()]),
  );

  const sourceFile = ts.createSourceFile(ROUTES_FILE, content, ts.ScriptTarget.ES2022, true);
  for (const stmt of sourceFile.statements) {
    if (!ts.isInterfaceDeclaration(stmt) || stmt.name.text !== 'Routes') continue;
    for (const member of stmt.members) {
      if (!ts.isPropertySignature(member) || !member.type || !ts.isTypeLiteralNode(member.type)) {
        continue;
      }
      const routeKey = member.name.getText(sourceFile).replace(/^['"]|['"]$/g, '');
      for (const inner of member.type.members) {
        if (!ts.isPropertySignature(inner)) continue;
        const fieldName = inner.name.getText(sourceFile).replace(/^['"]|['"]$/g, '');
        if (fieldName in FIELD_TO_MAP) {
          routeFields.get(fieldName)?.add(routeKey);
        }
      }
    }
  }

  return routeFields;
}

function parseSchemaMaps(content) {
  const mapEntries = new Map(
    Object.values(FIELD_TO_MAP).map((mapName) => [mapName, new Set()]),
  );

  const sourceFile = ts.createSourceFile(SCHEMAS_FILE, content, ts.ScriptTarget.ES2022, true);
  for (const stmt of sourceFile.statements) {
    if (!ts.isVariableStatement(stmt)) continue;
    for (const decl of stmt.declarationList.declarations) {
      if (!ts.isIdentifier(decl.name) || !mapEntries.has(decl.name.text) || !decl.initializer) {
        continue;
      }
      const initializer = ts.isAsExpression(decl.initializer)
        ? decl.initializer.expression
        : decl.initializer;
      if (!ts.isObjectLiteralExpression(initializer)) continue;
      const entries = mapEntries.get(decl.name.text);
      for (const property of initializer.properties) {
        if (!ts.isPropertyAssignment(property) && !ts.isShorthandPropertyAssignment(property)) {
          continue;
        }
        const key = property.name.getText(sourceFile).replace(/^['"]|['"]$/g, '');
        entries?.add(key);
      }
    }
  }

  return mapEntries;
}

const routeFields = parseRouteFields(fs.readFileSync(ROUTES_FILE, 'utf-8'));
const schemaMaps = parseSchemaMaps(fs.readFileSync(SCHEMAS_FILE, 'utf-8'));

let violations = 0;

for (const [field, mapName] of Object.entries(FIELD_TO_MAP)) {
  const requiredKeys = routeFields.get(field) ?? new Set();
  const mappedKeys = schemaMaps.get(mapName) ?? new Set();

  for (const key of [...requiredKeys].filter((routeKey) => !mappedKeys.has(routeKey)).sort()) {
    console.error(
      `daemon-contract-schema-coverage: route "${key}" declares ${field} in routes.ts but is missing from ${mapName} in schemas.ts. Run npm run gen:schemas and keep the generated schema map complete.`,
    );
    violations++;
  }

  for (const key of [...mappedKeys].filter((routeKey) => !requiredKeys.has(routeKey)).sort()) {
    console.error(
      `daemon-contract-schema-coverage: ${mapName} contains "${key}" but routes.ts does not declare ${field} for that route. Remove the stale generated entry or regenerate the contract output.`,
    );
    violations++;
  }
}

if (violations > 0) {
  console.error(`\n${violations} daemon-contract schema coverage violation(s) found.`);
  process.exit(1);
}
