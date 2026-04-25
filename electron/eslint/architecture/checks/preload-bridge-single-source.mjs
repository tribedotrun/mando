#!/usr/bin/env node
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PRELOAD_TYPES_DIR = path.resolve(__dirname, '../../../src/preload/types');

if (!fs.existsSync(PRELOAD_TYPES_DIR)) {
  process.exit(0);
}

const files = fs
  .readdirSync(PRELOAD_TYPES_DIR, { withFileTypes: true })
  .filter((entry) => entry.isFile() && /\.(ts|tsx)$/.test(entry.name) && !/\.d\.ts$/.test(entry.name));

if (files.length === 0) {
  process.exit(0);
}

for (const entry of files) {
  console.error(
    `preload-bridge-single-source: src/preload/types/${entry.name} mirrors the native bridge. Keep the authored preload surface in src/preload/providers/ipc.ts and export its type directly instead of adding preload type mirrors.`,
  );
}
console.error(`
${files.length} preload-bridge-single-source violation(s) found.`);
process.exit(1);
