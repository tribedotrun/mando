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
  validateLinearKey: (apiKey: string) => Promise<{
    valid: boolean;
    teams: Array<{ id: string; key: string; name: string }>;
    error?: string;
  }>;
  // Config & setup (proxied through main process to daemon HTTP)
  gatewayUrl: () => Promise<string>;
  appInfo: () => Promise<{
    appVersion: string;
    stack: Array<{ name: string; version: string }>;
  }>;
  dataDir: () => Promise<string>;
  hasConfig: () => Promise<boolean>;
  configPath: () => Promise<string>;
  readConfig: () => Promise<string>;
  saveConfig: (config: string) => Promise<boolean>;
  addProject: (body: string) => Promise<{
    ok: boolean;
    name: string;
    path: string;
    githubRepo: string;
  }>;
  saveConfigLocal: (config: string) => Promise<boolean>;
  setupComplete: (config: string) => Promise<boolean>;
  onSetupProgress: (callback: (step: string) => void) => void;
  // Connection state
  connectionState: () => Promise<string>;
  onConnectionState: (callback: (state: string) => void) => void;
  removeConnectionStateListeners: () => void;
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
    installUpdate: () => Promise<void>;
    checkForUpdates: () => Promise<void>;
    getPending: () => Promise<{ version: string; notes: string } | null>;
    appVersion: () => Promise<string>;
    getChannel: () => Promise<string>;
    setChannel: (channel: string) => Promise<void>;
    removeUpdateListeners: () => void;
  };
  // Voice window
  hideVoiceWindow: () => void;
  onVoiceStartRecording: (callback: () => void) => void;
  onVoiceStopRecording: (callback: () => void) => void;
  removeVoiceListeners: () => void;
  // File dialogs
  selectDirectory: () => Promise<string | null>;
  // Login item
  setLoginItem: (enabled: boolean) => Promise<void>;
  // DevTools
  toggleDevTools: () => Promise<void>;
  // Logs
  openLogsFolder: () => void;
  // Launchd (macOS)
  launchd: {
    reinstall: () => Promise<boolean>;
    daemonStatus: () => Promise<{ loaded: boolean; running: boolean; pid: number | null }>;
  };
}

contextBridge.exposeInMainWorld('mandoAPI', {
  // App mode
  appMode: () => ipcRenderer.invoke('get-app-mode'),
  devGitInfo: () => ipcRenderer.invoke('get-dev-git-info'),
  // System checks
  checkClaudeCode: () => ipcRenderer.invoke('check-claude-code'),
  validateTelegramToken: (token: string) => ipcRenderer.invoke('validate-telegram-token', token),
  validateLinearKey: (apiKey: string) => ipcRenderer.invoke('validate-linear-key', apiKey),
  // Config & setup
  gatewayUrl: () => ipcRenderer.invoke('get-gateway-url'),
  appInfo: () => ipcRenderer.invoke('get-app-info'),
  dataDir: () => ipcRenderer.invoke('get-data-dir'),
  hasConfig: () => ipcRenderer.invoke('has-config'),
  configPath: () => ipcRenderer.invoke('get-config-path'),
  readConfig: () => ipcRenderer.invoke('read-config'),
  saveConfig: (config: string) => ipcRenderer.invoke('save-config', config),
  addProject: (body: string) => ipcRenderer.invoke('add-project', body),
  saveConfigLocal: (config: string) => ipcRenderer.invoke('save-config-local', config),
  setupComplete: (config: string) => ipcRenderer.invoke('setup-complete', config),
  onSetupProgress: (callback: (step: string) => void) => {
    ipcRenderer.on('setup-progress', (_event, step: string) => callback(step));
  },
  // Connection state
  connectionState: () => ipcRenderer.invoke('get-connection-state'),
  onConnectionState: (callback: (state: string) => void) => {
    ipcRenderer.on('connection-state', (_event, state: string) => callback(state));
  },
  removeConnectionStateListeners: () => {
    ipcRenderer.removeAllListeners('connection-state');
  },
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
  },
  // Voice window
  hideVoiceWindow: () => ipcRenderer.send('hide-voice-window'),
  onVoiceStartRecording: (callback: () => void) => {
    ipcRenderer.on('voice-start-recording', () => callback());
  },
  onVoiceStopRecording: (callback: () => void) => {
    ipcRenderer.on('voice-stop-recording', () => callback());
  },
  removeVoiceListeners: () => {
    ipcRenderer.removeAllListeners('voice-start-recording');
    ipcRenderer.removeAllListeners('voice-stop-recording');
  },
  // Login item
  selectDirectory: () => ipcRenderer.invoke('select-directory'),
  setLoginItem: (enabled: boolean) => ipcRenderer.invoke('set-login-item', enabled),
  // DevTools
  toggleDevTools: () => ipcRenderer.invoke('toggle-devtools'),
  // Logs
  openLogsFolder: () => ipcRenderer.invoke('open-logs-folder'),
  // Launchd (macOS)
  launchd: {
    reinstall: () => ipcRenderer.invoke('launchd:reinstall'),
    daemonStatus: () => ipcRenderer.invoke('launchd:daemon-status'),
  },
} satisfies MandoAPI);
