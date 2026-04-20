// Renderer uses HTTP to the daemon for all data operations.
// Only Electron-native operations are exposed via IPC.
//
// Every method delegates to `invoke()` / `subscribe()` from #shared/ipc-contract,
// which parses the result/payload against the channel's Zod schema before handing
// it back. The types below are kept in step with the channel schemas; if they drift,
// the runtime parser throws and CI catches it.

import type { NotificationKind, NotificationPayload } from '#shared/notifications';

export type { NotificationPayload } from '#shared/notifications';

// Mirror the UpdateChannel union here (instead of importing from #main) since
// preload cannot reach into main per the tier architecture. Kept in sync with
// updateChannelSchema in #main/updater/types/updater.
export type UpdateChannel = 'stable' | 'beta';

export interface MandoAPI {
  // App mode: 'production' | 'dev' | 'sandbox'
  appMode: () => Promise<string>;
  // Dev-only: git branch + worktree name
  devGitInfo: () => Promise<{
    branch: string;
    commit: string;
    worktree: string | null;
    slot: string | null;
  }>;
  // System checks
  checkClaudeCode: () => Promise<{ installed: boolean; version: string | null; works: boolean }>;
  validateTelegramToken: (
    token: string,
  ) => Promise<{ valid: boolean; botName?: string; botUsername?: string; error?: string }>;
  // Config & setup (proxied through main process to daemon HTTP)
  // Returns null when the gateway port file is unreadable (daemon not yet started).
  gatewayUrl: () => Promise<string | null>;
  appInfo: () => Promise<{
    appVersion: string;
    stack: Array<{ name: string; version: string }>;
  }>;
  hasConfig: () => Promise<boolean>;
  readConfig: () => Promise<string>;
  // saveConfig removed -- renderer calls PUT /api/config directly
  // addProject removed -- renderer calls POST /api/projects directly
  saveConfigLocal: (config: string) => Promise<boolean>;
  setupComplete: (config: string) => Promise<{
    ok: boolean;
    daemonNotified: boolean;
    launchdInstalled: boolean;
    error?: string;
  }>;
  onSetupProgress: (callback: (step: string) => void) => () => void;
  // Daemon control
  restartDaemon: () => Promise<boolean>;
  // Shortcuts
  onShortcut: (callback: (action: string) => void) => () => void;
  // Desktop notifications. Payload is the wire NotificationPayload; the IPC contract
  // parses it on receipt before dispatching the native notification.
  showNotification: (payload: NotificationPayload) => void;
  onNotificationClick: (
    callback: (data: { kind: NotificationKind; item_id?: string }) => void,
  ) => () => void;
  // Auto-update
  updates: {
    onUpdateReady: (callback: (info: { version: string; notes: string }) => void) => () => void;
    onUpdateChecking: (callback: () => void) => () => void;
    onUpdateNoUpdate: (callback: () => void) => () => void;
    onUpdateCheckError: (callback: () => void) => () => void;
    onUpdateCheckDone: (callback: (info: { found: boolean }) => void) => () => void;
    installUpdate: () => Promise<void>;
    checkForUpdates: () => Promise<void>;
    getPending: () => Promise<{ version: string; notes: string } | null>;
    appVersion: () => Promise<string>;
    getChannel: () => Promise<UpdateChannel>;
    setChannel: (channel: UpdateChannel) => Promise<void>;
  };
  // File dialogs
  selectDirectory: () => Promise<string | null>;
  // Login item
  setLoginItem: (enabled: boolean) => Promise<void>;
  // DevTools
  toggleDevTools: () => Promise<void>;
  // Logs
  openLogsFolder: () => void;
  // Terminal desktop bridge
  openExternalUrl: (url: string) => Promise<void>;
  resolveLocalPath: (input: string, cwd: string) => Promise<string | null>;
  openLocalPath: (path: string) => Promise<void>;
  // Open paths
  openDataDir: () => void;
  openConfigFile: () => void;
  openInFinder: (dir: string) => Promise<void>;
  openInCursor: (dir: string) => Promise<void>;
}
