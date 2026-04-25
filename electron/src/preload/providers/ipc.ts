// Preload-side IPC bridge. This is the one authored renderer-facing surface for
// Electron-native capabilities; its type is exported directly from this module
// so we do not keep a second handwritten mirror under preload/types/.
//
// Every invoke goes through `invoke()` from shared/ipc-contract/runtime which
// parses the result against the channel's Zod schema before handing it back.
// Every subscription uses `subscribe()` which parses each pushed payload before
// calling the renderer's callback.
//
// Subscription contract (#831): every `on*` method returns a caller-owned
// unsubscribe function. The caller is responsible for invoking it on cleanup;
// the bridge exposes no channel-wide `remove*Listeners` APIs.

import { invoke, send, subscribe, type PayloadOf, type ResultOf } from '#shared/ipc-contract';

export type UpdateChannel = ResultOf<'updates:get-channel'>;

const updatesApi = {
  onUpdateReady: (callback: (info: { version: string; notes: string }) => void) =>
    subscribe('update-ready', callback),
  onUpdateChecking: (callback: () => void) => subscribe('update-checking', () => callback()),
  onUpdateNoUpdate: (callback: () => void) => subscribe('update-no-update', () => callback()),
  onUpdateCheckError: (callback: () => void) => subscribe('update-check-error', () => callback()),
  onUpdateCheckDone: (callback: (info: { found: boolean }) => void) =>
    subscribe('update-check-done', callback),
  installUpdate: () => invoke('updates:install'),
  checkForUpdates: () => invoke('updates:check'),
  getPending: () => invoke('updates:pending'),
  appVersion: () => invoke('updates:app-version'),
  getChannel: () => invoke('updates:get-channel'),
  setChannel: (channel: UpdateChannel) => invoke('updates:set-channel', channel),
} as const;

export const ipcApi = {
  appMode: () => invoke('get-app-mode'),
  devGitInfo: () => invoke('get-dev-git-info'),
  checkClaudeCode: () => invoke('check-claude-code'),
  validateTelegramToken: (token: string) => invoke('validate-telegram-token', token),
  gatewayUrl: () => invoke('get-gateway-url'),
  appInfo: () => invoke('get-app-info'),
  hasConfig: () => invoke('has-config'),
  readConfig: () => invoke('read-config'),
  saveConfigLocal: (config: string) => invoke('save-config-local', config),
  setupComplete: (config: string) => invoke('setup-complete', config),
  onSetupProgress: (callback: (step: string) => void) => subscribe('setup-progress', callback),
  restartDaemon: () => invoke('restart-daemon'),
  onShortcut: (callback: (action: string) => void) => subscribe('shortcut', callback),
  showNotification: (payload: PayloadOf<'show-notification'>) => {
    send('show-notification', payload);
  },
  onNotificationClick: (callback: (data: PayloadOf<'notification-click'>) => void) =>
    subscribe('notification-click', callback),
  updates: updatesApi,
  selectDirectory: () => invoke('select-directory'),
  setLoginItem: (enabled: boolean) => invoke('set-login-item', enabled),
  toggleDevTools: () => invoke('toggle-devtools'),
  openLogsFolder: () => {
    void invoke('open-logs-folder');
  },
  openExternalUrl: (url: string) => invoke('terminal:open-external-url', url),
  resolveLocalPath: (input: string, cwd: string) =>
    invoke('terminal:resolve-local-path', [input, cwd]),
  openLocalPath: (path: string) => invoke('terminal:open-local-path', path),
  openDataDir: () => {
    void invoke('open-data-dir');
  },
  openConfigFile: () => {
    void invoke('open-config-file');
  },
  openInFinder: (dir: string) => invoke('open-in-finder', dir),
  openInCursor: (dir: string) => invoke('open-in-cursor', dir),
} as const;

export type MandoAPI = typeof ipcApi;
