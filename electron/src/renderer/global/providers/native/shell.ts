export function openInFinder(path: string) {
  return window.mandoAPI.openInFinder(path);
}

export function openInCursor(path: string) {
  return window.mandoAPI.openInCursor(path);
}

export function selectDirectory() {
  return window.mandoAPI.selectDirectory();
}

export function openLogsFolder(): void {
  window.mandoAPI.openLogsFolder();
}

export function openConfigFile(): void {
  window.mandoAPI.openConfigFile();
}

export function openDataDir(): void {
  window.mandoAPI.openDataDir();
}

export function toggleDevTools() {
  return window.mandoAPI.toggleDevTools();
}

export function openExternalUrl(url: string) {
  return window.mandoAPI.openExternalUrl(url);
}

export function resolveLocalPath(input: string, cwd: string) {
  return window.mandoAPI.resolveLocalPath(input, cwd);
}

export function openLocalPath(path: string) {
  return window.mandoAPI.openLocalPath(path);
}
