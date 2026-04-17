#!/usr/bin/env node
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SRC = path.resolve(__dirname, '../../../src');

const DOMAIN_IMPORT_RE = /from\s+['"]#renderer\/domains\/([^/]+)\/(?!index['"])((?:types|config|repo|service|runtime|ui|terminal)\/[^'"]+)['"]/g;

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

function isAppFile(filePath) {
  return path.relative(SRC, filePath).replaceAll('\\', '/').startsWith('renderer/app/');
}

const domainFiles = allTsFiles(path.join(SRC, 'renderer', 'domains'));
const appFiles = allTsFiles(path.join(SRC, 'renderer', 'app'));
const allFiles = [...domainFiles, ...appFiles];

const domainInternalConsumers = new Map();

for (const file of allFiles) {
  const content = fs.readFileSync(file, 'utf-8');
  const sourceDomain = getDomain(file);
  const isApp = isAppFile(file);

  let match;
  while ((match = DOMAIN_IMPORT_RE.exec(content)) !== null) {
    const targetDomain = match[1];
    const targetPath = match[2];
    const key = `${targetDomain}/${targetPath}`;

    if (isApp) continue;

    if (sourceDomain && sourceDomain !== targetDomain) {
      if (!domainInternalConsumers.has(key)) {
        domainInternalConsumers.set(key, new Set());
      }
      domainInternalConsumers.get(key).add(sourceDomain);
    }
  }
}

let violations = 0;
for (const [targetPath, consumers] of domainInternalConsumers) {
  if (consumers.size > 0) {
    const consumerList = [...consumers].join(', ');
    console.error(`cross-domain-leak: #renderer/domains/${targetPath} is imported by domain(s): ${consumerList}. Promote to global or remove the cross-domain dependency. See s-arch skill.`);
    violations++;
  }
}

if (violations > 0) {
  console.error(`\n${violations} cross-domain-leak violation(s) found.`);
  process.exit(1);
}
