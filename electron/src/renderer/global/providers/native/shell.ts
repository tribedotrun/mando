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

export function openLocalPath(path: string) {
  return window.mandoAPI.openLocalPath(path);
}

export function evidenceDeckExists(worktree: string) {
  return window.mandoAPI.evidenceDeckExists(worktree);
}

export function readEvidenceDeck(worktree: string) {
  return window.mandoAPI.readEvidenceDeck(worktree);
}
