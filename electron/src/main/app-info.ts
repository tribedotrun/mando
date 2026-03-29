import { app } from 'electron';
import fs from 'fs';
import path from 'path';
import log from '#main/logger';

interface AppStackItem {
  name: string;
  version: string;
}

interface AppInfo {
  appVersion: string;
  stack: AppStackItem[];
}

function readElectronPackageJson(): Record<string, unknown> {
  const candidatePaths = [
    path.join(app.getAppPath(), 'package.json'),
    path.resolve(app.getAppPath(), '..', 'package.json'),
    path.resolve(__dirname, '../../package.json'),
  ];
  for (const candidate of candidatePaths) {
    try {
      if (!fs.existsSync(candidate)) continue;
      return JSON.parse(fs.readFileSync(candidate, 'utf-8')) as Record<string, unknown>;
    } catch (e) {
      log.warn(`Failed to read package.json from ${candidate}:`, e);
      continue;
    }
  }
  log.warn('No readable package.json found in any candidate path');
  return {};
}

function dependencyVersion(pkg: Record<string, unknown>, key: string): string {
  const sections = ['dependencies', 'devDependencies'] as const;
  for (const section of sections) {
    const value = pkg[section];
    if (!value || typeof value !== 'object') continue;
    const version = (value as Record<string, string>)[key];
    if (version) return version;
  }
  return '';
}

export function getAppInfo(): AppInfo {
  const pkg = readElectronPackageJson();
  return {
    appVersion: app.getVersion(),
    stack: [
      { name: 'Electron', version: process.versions.electron ?? '' },
      { name: 'React', version: dependencyVersion(pkg, 'react') },
      { name: 'TypeScript', version: dependencyVersion(pkg, 'typescript') },
      { name: 'Tailwind CSS', version: dependencyVersion(pkg, 'tailwindcss') },
      { name: 'Zustand', version: dependencyVersion(pkg, 'zustand') },
    ],
  };
}
