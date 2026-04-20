// Typed funnel for every `window.mandoAPI.*` call in the renderer.
//
// Everything outside `**/providers/**` imports from here; direct access to
// `window.mandoAPI` is ESLint-banned (R8 / ipc-only-in-providers). Keep this
// module the single place that reaches across the native bridge so that
// renderer code is insulated from IPC-vs-HTTP concerns.

import type { UpdateChannel } from '#preload/types/api';
import type { NotificationKind, NotificationPayload } from '#shared/notifications';

function ipc() {
  if (typeof window === 'undefined' || !window.mandoAPI) {
    // invariant: programmer bug — callers must guard with hasIpcBridge() when the bridge may be absent (early bootstrap, tests)
    throw new Error('window.mandoAPI is not available');
  }
  return window.mandoAPI;
}

export function hasIpcBridge(): boolean {
  return typeof window !== 'undefined' && !!window.mandoAPI;
}

// -- App / Environment --
export function appMode() {
  return ipc().appMode();
}
export function devGitInfo() {
  return ipc().devGitInfo();
}
export function gatewayUrl() {
  return ipc().gatewayUrl();
}
export function appInfo() {
  return ipc().appInfo();
}

// -- System checks --
export function checkClaudeCode() {
  return ipc().checkClaudeCode();
}
export function validateTelegramToken(token: string) {
  return ipc().validateTelegramToken(token);
}

// -- Config & setup --
export function hasConfig() {
  return ipc().hasConfig();
}
export function readConfig() {
  return ipc().readConfig();
}
export function saveConfigLocal(config: string) {
  return ipc().saveConfigLocal(config);
}
export function setupComplete(config: string) {
  return ipc().setupComplete(config);
}
export function onSetupProgress(callback: (step: string) => void): () => void {
  return ipc().onSetupProgress(callback);
}

// -- Daemon control --
export function restartDaemon() {
  return ipc().restartDaemon();
}

// -- Shortcuts / notifications --
export function onShortcut(callback: (action: string) => void): () => void {
  return ipc().onShortcut(callback);
}
export function showNotification(payload: NotificationPayload): void {
  ipc().showNotification(payload);
}
export function onNotificationClick(
  callback: (data: { kind: NotificationKind; item_id?: string }) => void,
): () => void {
  return ipc().onNotificationClick(callback);
}

// -- Auto-update --
export const updates = {
  onUpdateReady: (callback: (info: { version: string; notes: string }) => void) =>
    ipc().updates.onUpdateReady(callback),
  onUpdateChecking: (callback: () => void) => ipc().updates.onUpdateChecking(callback),
  onUpdateNoUpdate: (callback: () => void) => ipc().updates.onUpdateNoUpdate(callback),
  onUpdateCheckError: (callback: () => void) => ipc().updates.onUpdateCheckError(callback),
  onUpdateCheckDone: (callback: (info: { found: boolean }) => void) =>
    ipc().updates.onUpdateCheckDone(callback),
  installUpdate: () => ipc().updates.installUpdate(),
  checkForUpdates: () => ipc().updates.checkForUpdates(),
  getPending: () => ipc().updates.getPending(),
  appVersion: () => ipc().updates.appVersion(),
  getChannel: () => ipc().updates.getChannel(),
  setChannel: (channel: UpdateChannel) => ipc().updates.setChannel(channel),
};

// -- File dialogs --
export function selectDirectory() {
  return ipc().selectDirectory();
}

// -- Login item --
export function setLoginItem(enabled: boolean) {
  return ipc().setLoginItem(enabled);
}

// -- DevTools --
export function toggleDevTools() {
  return ipc().toggleDevTools();
}

// -- Logs --
export function openLogsFolder() {
  return ipc().openLogsFolder();
}

// -- Terminal desktop bridge --
export function openExternalUrl(url: string) {
  return ipc().openExternalUrl(url);
}
export function resolveLocalPath(input: string, cwd: string) {
  return ipc().resolveLocalPath(input, cwd);
}
export function openLocalPath(path: string) {
  return ipc().openLocalPath(path);
}

// -- Open paths --
export function openDataDir() {
  return ipc().openDataDir();
}
export function openConfigFile() {
  return ipc().openConfigFile();
}
export function openInFinder(dir: string) {
  return ipc().openInFinder(dir);
}
export function openInCursor(dir: string) {
  return ipc().openInCursor(dir);
}
