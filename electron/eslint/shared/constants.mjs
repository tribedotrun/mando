import { readdirSync } from 'node:fs';
import { resolve } from 'node:path';

const SRC_DIR = resolve(import.meta.dirname, '../../src');

function childDirectories(directory, excluded = new Set()) {
  return readdirSync(directory, { withFileTypes: true })
    .filter((entry) => entry.isDirectory() && !excluded.has(entry.name))
    .map((entry) => entry.name)
    .sort();
}

// Renderer domains live below renderer/domains. Main domains are siblings of
// main/global, so excluding that shared column leaves the domain registry.
export const RENDERER_DOMAINS = childDirectories(resolve(SRC_DIR, 'renderer/domains'));
export const MAIN_DOMAINS = childDirectories(resolve(SRC_DIR, 'main'), new Set(['global']));
export const RENDERER_TIERS = ['types', 'config', 'providers', 'repo', 'service', 'runtime', 'ui'];
export const MAIN_TIERS = ['types', 'config', 'providers', 'repo', 'service', 'runtime'];

export const ALL_TS = ['src/**/*.ts', 'src/**/*.tsx'];
export const RENDERER_TS = ['src/renderer/**/*.ts', 'src/renderer/**/*.tsx'];
export const MAIN_TS = ['src/main/**/*.ts'];
export const PRELOAD_TS = ['src/preload/**/*.ts'];
