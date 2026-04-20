// Preload-side IPC bridge. Every invoke goes through `invoke()` from
// shared/ipc-contract/runtime which parses the result against the channel's
// Zod schema before handing it back. Every subscription uses `subscribe()`
// which parses each pushed payload before calling the renderer's callback.
//
// Subscription contract (#831): every `on*` method returns a caller-owned
// unsubscribe function. The caller is responsible for invoking it on
// cleanup; the bridge exposes no channel-wide `remove*Listeners` APIs.
// This eliminates the previous pattern where one feature could tear down
// another feature's listener by calling the wrong cleanup.

import type { MandoAPI } from '#preload/types/api';
import { invoke, subscribe, send } from '#shared/ipc-contract';

export const ipcApi: MandoAPI = {
  // App mode
  appMode: () => invoke('get-app-mode'),
  devGitInfo: () => invoke('get-dev-git-info'),
  // System checks
  checkClaudeCode: () => invoke('check-claude-code'),
  validateTelegramToken: (token) => invoke('validate-telegram-token', token),
  // Config & setup
  gatewayUrl: () => invoke('get-gateway-url'),
  appInfo: () => invoke('get-app-info'),
  hasConfig: () => invoke('has-config'),
  readConfig: () => invoke('read-config'),
  saveConfigLocal: (config) => invoke('save-config-local', config),
  setupComplete: (config) => invoke('setup-complete', config),
  onSetupProgress: (callback) => subscribe('setup-progress', callback),
  // Daemon control
  restartDaemon: () => invoke('restart-daemon'),
  // Shortcuts
  onShortcut: (callback) => subscribe('shortcut', callback),
  // Desktop notifications
  showNotification: (payload) => {
    send('show-notification', payload);
  },
  onNotificationClick: (callback) => subscribe('notification-click', callback),
  // Auto-update
  updates: {
    onUpdateReady: (callback) => subscribe('update-ready', callback),
    onUpdateChecking: (callback) => subscribe('update-checking', callback),
    onUpdateNoUpdate: (callback) => subscribe('update-no-update', callback),
    onUpdateCheckError: (callback) => subscribe('update-check-error', callback),
    onUpdateCheckDone: (callback) => subscribe('update-check-done', callback),
    installUpdate: () => invoke('updates:install'),
    checkForUpdates: () => invoke('updates:check'),
    getPending: () => invoke('updates:pending'),
    appVersion: () => invoke('updates:app-version'),
    getChannel: () => invoke('updates:get-channel'),
    setChannel: (channel) => invoke('updates:set-channel', channel),
  },
  // Native shell
  selectDirectory: () => invoke('select-directory'),
  setLoginItem: (enabled) => invoke('set-login-item', enabled),
  toggleDevTools: () => invoke('toggle-devtools'),
  openLogsFolder: () => {
    void invoke('open-logs-folder');
  },
  openExternalUrl: (url) => invoke('terminal:open-external-url', url),
  resolveLocalPath: (input, cwd) => invoke('terminal:resolve-local-path', [input, cwd]),
  openLocalPath: (path) => invoke('terminal:open-local-path', path),
  openDataDir: () => {
    void invoke('open-data-dir');
  },
  openConfigFile: () => {
    void invoke('open-config-file');
  },
  openInFinder: (dir) => invoke('open-in-finder', dir),
  openInCursor: (dir) => invoke('open-in-cursor', dir),
};
