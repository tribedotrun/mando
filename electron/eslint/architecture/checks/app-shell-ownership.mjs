#!/usr/bin/env node
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SRC = path.resolve(__dirname, '../../../src/renderer/app');

const SHELL_FILE_PATTERNS = [
  /^AppHeader[A-Za-z0-9_-]*\.tsx$/,
  /^Sidebar(?:[A-Za-z0-9_-]*)\.tsx$/,
  /^routes\/AppLayout\.tsx$/,
  /^routes\/RootFrame\.tsx$/,
  /^routes\/RootShellOverlays\.tsx$/,
];

const DISALLOWED_DOMAIN_IMPORT_RE = /from ['"]#renderer\/domains\/(?![^'"]+\/shell['"])/;

function allTsFiles(dir) {
  const results = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      results.push(...allTsFiles(full));
      continue;
    }
    if (!/\.(ts|tsx)$/.test(entry.name) || /\.d\.ts$/.test(entry.name)) continue;
    results.push(full);
  }
  return results;
}

let violations = 0;
for (const file of allTsFiles(SRC)) {
  const rel = path.relative(SRC, file).replaceAll('\\', '/');
  if (!SHELL_FILE_PATTERNS.some((pattern) => pattern.test(rel))) continue;
  const text = fs.readFileSync(file, 'utf-8');
  if (!DISALLOWED_DOMAIN_IMPORT_RE.test(text)) continue;
  console.error(
    `app-shell-ownership: renderer/app/${rel} imports a domain-owned surface directly. App shell files must depend on app/global-owned shell adapters, not #renderer/domains/* imports. See s-arch invariants.`,
  );
  violations++;
}

if (violations > 0) {
  console.error(`
${violations} app-shell-ownership violation(s) found.`);
  process.exit(1);
}
