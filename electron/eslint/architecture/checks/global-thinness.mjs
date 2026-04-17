#!/usr/bin/env node
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SRC = path.resolve(__dirname, '../../../src');

const GLOBAL_IMPORT_RE = /from\s+['"]#renderer\/global\/([^'"]+)['"]/g;

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

function getDomain(filePath) {
  const rel = path.relative(SRC, filePath).replaceAll('\\', '/');
  const m = rel.match(/^renderer\/domains\/([^/]+)\//);
  return m ? m[1] : null;
}

function isGlobalFile(filePath) {
  return path.relative(SRC, filePath).replaceAll('\\', '/').startsWith('renderer/global/');
}

function isAppFile(filePath) {
  return path.relative(SRC, filePath).replaceAll('\\', '/').startsWith('renderer/app/');
}

const files = allTsFiles(path.join(SRC, 'renderer'));
const globalConsumers = new Map();

for (const file of files) {
  const content = fs.readFileSync(file, 'utf-8');
  const domain = getDomain(file);
  const isApp = isAppFile(file);
  const isGlobal = isGlobalFile(file);

  let match;
  while ((match = GLOBAL_IMPORT_RE.exec(content)) !== null) {
    const globalPath = match[1];
    if (!globalConsumers.has(globalPath)) {
      globalConsumers.set(globalPath, { domains: new Set(), appConsumer: false, globalConsumer: false });
    }
    const entry = globalConsumers.get(globalPath);
    if (domain) entry.domains.add(domain);
    if (isApp) entry.appConsumer = true;
    if (isGlobal) entry.globalConsumer = true;
  }
}

let violations = 0;
for (const [globalPath, { domains, appConsumer, globalConsumer }] of globalConsumers) {
  if (globalPath.startsWith('ui/')) continue;
  if (globalConsumer) continue;
  if (domains.size === 1 && !appConsumer) {
    const [domain] = domains;
    console.error(`global-thinness: #renderer/global/${globalPath} has exactly one domain consumer ("${domain}"). Demote it to that domain. See s-arch skill.`);
    violations++;
  }
}

if (violations > 0) {
  console.error(`\n${violations} global-thinness violation(s) found.`);
  process.exit(1);
}
