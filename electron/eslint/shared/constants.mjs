export const RENDERER_DOMAINS = ['captain', 'scout', 'sessions', 'settings', 'onboarding'];
export const MAIN_DOMAINS = ['onboarding', 'daemon', 'updater', 'shell'];
export const RENDERER_TIERS = ['types', 'config', 'providers', 'repo', 'service', 'runtime', 'ui'];
export const MAIN_TIERS = ['types', 'config', 'providers', 'repo', 'service', 'runtime'];

export const ALL_TS = ['src/**/*.ts', 'src/**/*.tsx'];
export const RENDERER_TS = ['src/renderer/**/*.ts', 'src/renderer/**/*.tsx'];
export const MAIN_TS = ['src/main/**/*.ts'];
export const PRELOAD_TS = ['src/preload/**/*.ts'];

export const UI_FILE_GLOB = 'src/renderer/**/ui/**/*.tsx';

export function isUiFile(filename) {
  if (!filename) return false;
  return filename.replaceAll('\\', '/').includes('/ui/');
}

export function isServiceFile(filename) {
  if (!filename) return false;
  return filename.replaceAll('\\', '/').includes('/service/');
}

export function isAppFile(filename) {
  if (!filename) return false;
  return filename.replaceAll('\\', '/').includes('/renderer/app/');
}

export function isBarrelFile(filename) {
  if (!filename) return false;
  const norm = filename.replaceAll('\\', '/');
  return /\/domains\/[^/]+\/index\.ts$/.test(norm);
}

export function isTsxFile(filename) {
  if (!filename) return false;
  return filename.replaceAll('\\', '/').endsWith('.tsx');
}
