#!/usr/bin/env node
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SRC = path.resolve(__dirname, '../../../src');

const RELATIVE_IMPORT_RE = /from\s+['"](\.\.[^'"]*|\.\/[^'"]*)['"]/g;
const TIERS = new Set(['types', 'config', 'providers', 'repo', 'service', 'runtime', 'ui', 'ipc']);

function allTsFiles(dir) {
  const results = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory() && !['node_modules', 'dist', '.vite'].includes(entry.name)) {
      results.push(...allTsFiles(full));
    } else if (/\.(ts|tsx)$/.test(entry.name)) {
      results.push(full);
    }
  }
  return results;
}

function getTier(filePath) {
  const parts = filePath.replaceAll('\\', '/').split('/');
  for (const p of parts) {
    if (TIERS.has(p)) return p;
  }
  return null;
}

const SKIP = ['src/renderer/index.tsx'];

const files = allTsFiles(SRC);
let violations = 0;

for (const file of files) {
  const rel = path.relative(path.resolve(SRC, '..'), file).replaceAll('\\', '/');
  if (SKIP.some((s) => rel.endsWith(s))) continue;

  const content = fs.readFileSync(file, 'utf-8');
  const fileTier = getTier(file);
  if (!fileTier) continue;

  let match;
  while ((match = RELATIVE_IMPORT_RE.exec(content)) !== null) {
    const importPath = match[1];
    const resolved = path.resolve(path.dirname(file), importPath);
    const importTier = getTier(resolved);
    if (importTier && importTier !== fileTier) {
      const lineNum = content.substring(0, match.index).split('\n').length;
      console.error(`${rel}:${lineNum} cross-tier relative import from ${fileTier}/ to ${importTier}/: "${importPath}". Use path aliases. See s-arch skill.`);
      violations++;
    }
  }
}

if (violations > 0) {
  console.error(`\n${violations} cross-tier relative import(s) found.`);
  process.exit(1);
}
