import { app } from 'electron';
import fs from 'fs';
import path from 'path';
import log from '#main/global/providers/logger';

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
      return JSON.parse(fs.readFileSync(candidate, 'utf-8')) as Record<string, unknown>;
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
