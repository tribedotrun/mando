#!/usr/bin/env node
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SRC = path.resolve(__dirname, '../../../src');

const RULES = [
  {
    match: (rel) => rel.startsWith('main/') && rel.includes('/runtime/'),
    maxLines: 200,
    label: 'main runtime owner',
  },
  {
    match: (rel) => rel.startsWith('renderer/global/providers/'),
    maxLines: 300,
    label: 'renderer global provider',
  },
  {
    match: (rel) => rel.startsWith('renderer/global/runtime/'),
    maxLines: 200,
    label: 'renderer global runtime',
  },
  {
    match: (rel) => rel.startsWith('renderer/global/repo/'),
    maxLines: 200,
    label: 'renderer global repo',
  },
  {
    match: (rel) => rel.startsWith('renderer/global/service/'),
    maxLines: 350,
    label: 'renderer global service',
  },
  {
    match: (rel) => rel.startsWith('renderer/domains/') && rel.includes('/repo/'),
    maxLines: 400,
    label: 'renderer domain repo',
  },
  {
    match: (rel) => rel.startsWith('renderer/domains/') && rel.includes('/runtime/'),
    maxLines: 350,
    label: 'renderer domain runtime',
  },
];

function allTsFiles(dir) {
  const results = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory() && !['node_modules', 'dist', '.vite', '__tests__'].includes(entry.name)) {
      results.push(...allTsFiles(full));
    } else if (/\.(ts|tsx)$/.test(entry.name) && !/\.d\.ts$/.test(entry.name)) {
      results.push(full);
    }
  }
  return results;
}

function countLines(filePath) {
  return fs.readFileSync(filePath, 'utf-8').split('\n').length;
}

let violations = 0;
for (const file of allTsFiles(SRC)) {
  const rel = path.relative(SRC, file).replaceAll('\\', '/');
  const rule = RULES.find((candidate) => candidate.match(rel));
  if (!rule) continue;
  const lines = countLines(file);
  if (lines <= rule.maxLines) continue;
  console.error(
    `ownership-pressure: ${rel} is ${lines} lines. ${rule.label} files must stay at or below ${rule.maxLines} lines so one module keeps one dominant role. Split the responsibilities or move shared logic behind a narrower boundary. See s-arch invariants.`,
  );
  violations++;
}

if (violations > 0) {
  console.error(`\n${violations} ownership-pressure violation(s) found.`);
  process.exit(1);
}
