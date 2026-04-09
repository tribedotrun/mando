import { contextBridge, ipcRenderer } from 'electron';

// Renderer uses HTTP to the daemon for all data operations.
// Only Electron-native operations are exposed via IPC.
export interface MandoAPI {
  // App mode: 'production' | 'dev' | 'sandbox'
  appMode: () => Promise<string>;
  // Dev-only: git branch + worktree name
  devGitInfo: () => Promise<{ branch: string; worktree: string | null; slot: string | null }>;
  // System checks
  checkClaudeCode: () => Promise<{ installed: boolean; version: string | null; works: boolean }>;
  validateTelegramToken: (
    token: string,
  ) => Promise<{ valid: boolean; botName?: string; botUsername?: string; error?: string }>;
  // Config & setup (proxied through main process to daemon HTTP)
  gatewayUrl: () => Promise<string>;
  appInfo: () => Promise<{
    appVersion: string;
    stack: Array<{ name: string; version: string }>;
  }>;
  hasConfig: () => Promise<boolean>;
  readConfig: () => Promise<string>;
  // saveConfig removed — renderer calls PUT /api/config directly
  // addProject removed — renderer calls POST /api/projects directly
  saveConfigLocal: (config: string) => Promise<boolean>;
  setupComplete: (config: string) => Promise<{
    ok: boolean;
    daemonNotified: boolean;
    launchdInstalled: boolean;
    error?: string;
  }>;
  onSetupProgress: (callback: (step: string) => void) => void;
  // Daemon control
  restartDaemon: () => Promise<boolean>;
  // Shortcuts
  onShortcut: (callback: (action: string) => void) => void;
  removeShortcutListeners: () => void;
  // Desktop notifications
  showNotification: (payload: unknown) => void;
  onNotificationClick: (callback: (data: { kind: unknown; item_id?: string }) => void) => void;
  removeNotificationClickListeners: () => void;
  // Auto-update
  updates: {
    onUpdateReady: (callback: (info: { version: string; notes: string }) => void) => void;
    onUpdateChecking: (callback: () => void) => void;
    onUpdateNoUpdate: (callback: () => void) => void;
    onUpdateCheckError: (callback: () => void) => void;
    onUpdateCheckDone: (callback: (info: { found: boolean }) => void) => void;
    installUpdate: () => Promise<void>;
    checkForUpdates: () => Promise<void>;
    getPending: () => Promise<{ version: string; notes: string } | null>;
    appVersion: () => Promise<string>;
    getChannel: () => Promise<string>;
    setChannel: (channel: string) => Promise<void>;
    removeUpdateListeners: () => void;
    removeCheckListeners: () => void;
  };
  // File dialogs
  selectDirectory: () => Promise<string | null>;
  // Login item
  setLoginItem: (enabled: boolean) => Promise<void>;
  // DevTools
  toggleDevTools: () => Promise<void>;
  // Logs
  openLogsFolder: () => void;
  // Open paths
  openInFinder: (dir: string) => Promise<void>;
  openInCursor: (dir: string) => Promise<void>;
}

contextBridge.exposeInMainWorld('mandoAPI', {
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
  // saveConfig and addProject removed — renderer calls daemon HTTP directly
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
  // Open paths
  openInFinder: (dir: string) => ipcRenderer.invoke('open-in-finder', dir) as Promise<void>,
  openInCursor: (dir: string) => ipcRenderer.invoke('open-in-cursor', dir) as Promise<void>,
} satisfies MandoAPI);
