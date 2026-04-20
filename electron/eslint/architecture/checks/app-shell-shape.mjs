#!/usr/bin/env node
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const APP_DIR = path.resolve(__dirname, '../../../src/renderer/app');

const ALLOWED_TOP_LEVEL_FILE_PATTERNS = [
  /^router\.tsx$/,
  /^AppHeader[A-Za-z0-9_-]*\.tsx$/,
  /^Sidebar[A-Za-z0-9_-]*\.tsx$/,
  /^DataProvider[A-Za-z0-9_-]*\.tsx$/,
];

let violations = 0;

for (const entry of fs.readdirSync(APP_DIR, { withFileTypes: true })) {
  if (entry.isDirectory()) continue;
  if (!/\.(ts|tsx)$/.test(entry.name) || /\.d\.ts$/.test(entry.name)) continue;
  if (ALLOWED_TOP_LEVEL_FILE_PATTERNS.some((pattern) => pattern.test(entry.name))) continue;

  console.error(
    `app-shell-shape: renderer/app/${entry.name} is not an allowed top-level app-shell file. Keep top-level app/ for router wiring, DataProvider*, AppHeader*, and Sidebar* only; move domain-owned surfaces into their domain ui/. See s-arch app rules.`,
  );
  violations++;
}

if (violations > 0) {
  console.error(`\n${violations} app-shell-shape violation(s) found.`);
  process.exit(1);
}
