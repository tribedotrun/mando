import { app } from 'electron';
import fs from 'fs';
import path from 'path';
import { z } from 'zod';
import log from '#main/global/providers/logger';
import { parseJsonTextWith } from '#result';

// Only the fields we use. .passthrough() preserves the rest for callers that
// might add lookups later, while still rejecting non-object inputs.
const appPackageJsonSchema = z.object({ version: z.string().min(1).optional() }).passthrough();

function packageJsonCandidates(): string[] {
  const candidates = [
    path.join(app.getAppPath(), 'package.json'),
    path.resolve(app.getAppPath(), '..', 'package.json'),
    path.resolve(__dirname, '../../package.json'),
  ];

  if (app.isPackaged && process.resourcesPath) {
    candidates.unshift(path.join(process.resourcesPath, 'app.asar', 'package.json'));
  }

  return candidates;
}

export function readAppPackageJson(): Record<string, unknown> {
  for (const candidate of packageJsonCandidates()) {
    try {
      if (!fs.existsSync(candidate)) continue;
      const parsed = parseJsonTextWith(
        fs.readFileSync(candidate, 'utf-8'),
        appPackageJsonSchema,
        `file:${candidate}`,
      );
      if (parsed.isErr()) {
        log.warn(`package.json at ${candidate} failed schema parse`);
        continue;
      }
      return parsed.value;
    } catch (err) {
      log.warn(`Failed to read package.json from ${candidate}:`, err);
    }
  }

  log.warn('No readable package.json found in any candidate path');
  return {};
}

export function readAppPackageVersion(): string | null {
  const pkg = readAppPackageJson();
  const version = pkg.version;
  return typeof version === 'string' && version.length > 0 ? version : null;
}
