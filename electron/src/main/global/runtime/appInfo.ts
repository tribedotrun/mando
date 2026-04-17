import { app } from 'electron';
import { readAppPackageJson, readAppPackageVersion } from '#main/global/runtime/appPackage';

interface AppStackItem {
  name: string;
  version: string;
}

interface AppInfo {
  appVersion: string;
  stack: AppStackItem[];
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
  const pkg = readAppPackageJson();
  return {
    appVersion: readAppPackageVersion() ?? app.getVersion(),
    stack: [
      { name: 'Electron', version: process.versions.electron ?? '' },
      { name: 'React', version: dependencyVersion(pkg, 'react') },
      { name: 'TypeScript', version: dependencyVersion(pkg, 'typescript') },
      { name: 'Tailwind CSS', version: dependencyVersion(pkg, 'tailwindcss') },
      { name: 'Zustand', version: dependencyVersion(pkg, 'zustand') },
    ],
  };
}
