import { ipcRenderer } from 'electron';
import type { MandoAPI } from '#preload/types/api';

/** IPC wrapper functions that implement the MandoAPI contract. */
export const ipcApi: MandoAPI = {
  // App mode
  appMode: () => ipcRenderer.invoke('get-app-mode'),
  devGitInfo: () => ipcRenderer.invoke('get-dev-git-info'),
  // System checks
  checkClaudeCode: () => ipcRenderer.invoke('check-claude-code'),
  validateTelegramToken: (token: string) => ipcRenderer.invoke('validate-telegram-token', token),
  // Config & setup
  gatewayUrl: () => ipcRenderer.invoke('get-gateway-url'),
  appInfo: () => ipcRenderer.invoke('get-app-info'),
  hasConfig: () => ipcRenderer.invoke('has-config'),
  readConfig: () => ipcRenderer.invoke('read-config'),
  // saveConfig and addProject removed -- renderer calls daemon HTTP directly
  saveConfigLocal: (config: string) => ipcRenderer.invoke('save-config-local', config),
  setupComplete: (config: string) => ipcRenderer.invoke('setup-complete', config),
  onSetupProgress: (callback: (step: string) => void) => {
    ipcRenderer.on('setup-progress', (_event, step: string) => callback(step));
  },
  // Daemon control
  restartDaemon: () => ipcRenderer.invoke('restart-daemon'),
  // Shortcuts
  onShortcut: (callback: (action: string) => void) => {
    ipcRenderer.on('shortcut', (_event, action: string) => callback(action));
  },
  removeShortcutListeners: () => {
    ipcRenderer.removeAllListeners('shortcut');
  },
  // Desktop notifications
  showNotification: (payload: unknown) => {
    ipcRenderer.send('show-notification', payload);
  },
  onNotificationClick: (callback: (data: { kind: unknown; item_id?: string }) => void) => {
    ipcRenderer.on('notification-click', (_event, data) => callback(data));
  },
  removeNotificationClickListeners: () => {
    ipcRenderer.removeAllListeners('notification-click');
  },
  // Auto-update
  updates: {
    onUpdateReady: (callback: (info: { version: string; notes: string }) => void) => {
      ipcRenderer.on('update-ready', (_event, info) => callback(info));
    },
    onUpdateChecking: (callback: () => void) => {
      ipcRenderer.on('update-checking', () => callback());
    },
    onUpdateNoUpdate: (callback: () => void) => {
      ipcRenderer.on('update-no-update', () => callback());
    },
    onUpdateCheckError: (callback: () => void) => {
      ipcRenderer.on('update-check-error', () => callback());
    },
    onUpdateCheckDone: (callback: (info: { found: boolean }) => void) => {
      ipcRenderer.on('update-check-done', (_event, info) => callback(info));
    },
    installUpdate: () => ipcRenderer.invoke('updates:install'),
    checkForUpdates: () => ipcRenderer.invoke('updates:check'),
    getPending: () =>
      ipcRenderer.invoke('updates:pending') as Promise<{ version: string; notes: string } | null>,
    appVersion: () => ipcRenderer.invoke('updates:app-version'),
    getChannel: () => ipcRenderer.invoke('updates:get-channel'),
    setChannel: (channel: string) => ipcRenderer.invoke('updates:set-channel', channel),
    removeUpdateListeners: () => {
      ipcRenderer.removeAllListeners('update-ready');
    },
    removeCheckListeners: () => {
      ipcRenderer.removeAllListeners('update-checking');
      ipcRenderer.removeAllListeners('update-no-update');
      ipcRenderer.removeAllListeners('update-check-error');
      ipcRenderer.removeAllListeners('update-check-done');
    },
  },
  // Login item
  selectDirectory: () => ipcRenderer.invoke('select-directory'),
  setLoginItem: (enabled: boolean) => ipcRenderer.invoke('set-login-item', enabled),
  // DevTools
  toggleDevTools: () => ipcRenderer.invoke('toggle-devtools'),
  // Logs
  openLogsFolder: () => void ipcRenderer.invoke('open-logs-folder'),
  // Terminal desktop bridge
  openExternalUrl: (url: string) => ipcRenderer.invoke('terminal:open-external-url', url),
  resolveLocalPath: (input: string, cwd: string) =>
    ipcRenderer.invoke('terminal:resolve-local-path', input, cwd),
  openLocalPath: (path: string) => ipcRenderer.invoke('terminal:open-local-path', path),
  // Open paths
  openDataDir: () => void ipcRenderer.invoke('open-data-dir'),
  openConfigFile: () => void ipcRenderer.invoke('open-config-file'),
  openInFinder: (dir: string) => ipcRenderer.invoke('open-in-finder', dir) as Promise<void>,
  openInCursor: (dir: string) => ipcRenderer.invoke('open-in-cursor', dir) as Promise<void>,
};
