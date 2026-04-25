/**
 * Mando Electron main process -- composition-only bootstrap.
 *
 * No data operations: all data flows go through HTTP to the daemon.
 * No state held here: every lifecycle flow (window, tray, quit, daemon
 * connection, renderer server, login-item migration) lives in its own
 * owner module under `#main/global/runtime/`.
 *
 * Enforced by `architecture/main-composition-only`.
 */
import { app, BrowserWindow, dialog, globalShortcut } from 'electron';
import log from '#main/global/providers/logger';
import { registerConfigHandlers } from '#main/onboarding/repo/config';
import { registerSetupValidationHandlers } from '#main/onboarding/runtime/setupValidation';
import { getDevGitInfo } from '#main/global/runtime/devGitInfo';
import { installTrustedGatewayAuth } from '#main/daemon/runtime/gatewayAuth';
import {
  handleChannel,
  sendChannel,
  setTrustedRendererOrigins,
} from '#main/global/runtime/ipcSecurity';
import {
  readPort,
  ensureDaemon,
  startHealthMonitor,
  invalidateDiscoveryCache,
} from '#main/global/runtime/lifecycle';
import { getDataDir, getAppMode, isHeadless } from '#main/global/config/lifecycle';
import { createDockIcon } from '#main/global/runtime/icons';
import { registerNotificationHandlers } from '#main/shell/runtime/notifications';
import { registerTerminalBridgeHandlers } from '#main/shell/runtime/terminalBridge';
import { setupAutoUpdate, applyPendingUpdateIfAny } from '#main/updater/runtime/updater';
import { getAppInfo } from '#main/global/runtime/appInfo';
import { announceUiRegistered } from '#main/global/runtime/uiLifecycle';
import { isolateChromiumProfile } from '#main/global/runtime/appUserDataDir';
import {
  createMainWindow,
  getMainWindow,
  showAndFocusMainWindow,
  trustedRendererOrigins,
} from '#main/global/runtime/windowOwner';
import { installTray } from '#main/global/runtime/trayOwner';
import {
  prepareRendererSource,
  stopRendererServer,
} from '#main/global/runtime/rendererServerOwner';
import { isQuitRequested, runBeforeQuit } from '#main/global/runtime/quitController';
import { runLoginItemMigration } from '#main/global/runtime/loginItemMigration';
import { resolveAsset as resolveAssetService } from '#main/global/service/assetResolver';

isolateChromiumProfile();

// PR #883 invariant #1: route main-process uncaught exceptions and
// unhandled rejections through the structured logger before the process
// dies. Registering these listeners disables Node/Electron's default
// fatal-error behavior, so we must terminate explicitly — otherwise a
// crash leaves the app running in an undefined state and the launchd
// / supervisor restart never fires. Exit code 1 preserves crash
// semantics for whichever parent restarts the process.
process.on('uncaughtException', (err) => {
  log.error('uncaughtException — terminating', err);
  process.exit(1);
});
process.on('unhandledRejection', (reason) => {
  log.error('unhandledRejection — terminating', reason);
  process.exit(1);
});

const resolveAsset = (name: string): string => resolveAssetService(__dirname, name);

function registerShortcuts(): void {
  globalShortcut.register('CommandOrControl+N', () => {
    const win = getMainWindow();
    if (!win) return;
    sendChannel(win.webContents, 'shortcut', 'add-task');
  });
}

// ---------------------------------------------------------------------------
// IPC handlers -- config operations via daemon HTTP
// ---------------------------------------------------------------------------

handleChannel('get-gateway-url', async () => {
  let port = process.env.MANDO_GATEWAY_PORT;
  if (!port) {
    try {
      port = await readPort();
    } catch (err: unknown) {
      log.error('get-gateway-url: failed to read daemon port -- daemon may not be running:', err);
      return null;
    }
  }
  if (!port) return null;
  return `http://127.0.0.1:${port}`;
});

handleChannel('get-app-info', getAppInfo);

handleChannel('restart-daemon', async () => {
  invalidateDiscoveryCache();
  return ensureDaemon(getDataDir());
});
handleChannel('get-app-mode', () =>
  process.env.MANDO_HIDE_DEV_BAR === '1' ? 'clean' : getAppMode(),
);
handleChannel('select-directory', async () => {
  const opts = { properties: ['openDirectory' as const], message: 'Select a project folder' };
  const win = BrowserWindow.getFocusedWindow();
  const result = win ? await dialog.showOpenDialog(win, opts) : await dialog.showOpenDialog(opts);
  return result.canceled ? null : (result.filePaths[0] ?? null);
});
handleChannel('set-login-item', (_event, enabled) => {
  if (app.isPackaged) {
    app.setLoginItemSettings({ openAtLogin: enabled, openAsHidden: true });
  }
});

handleChannel('toggle-devtools', () => {
  getMainWindow()?.webContents.toggleDevTools();
});

handleChannel('get-dev-git-info', getDevGitInfo);

// Setup validation handlers (Claude Code, Telegram) -- see setup-validation.ts
registerSetupValidationHandlers();

// Config read/write, onboarding setup-complete, launchd -- see config-handlers.ts
registerConfigHandlers();
registerTerminalBridgeHandlers();

// ---------------------------------------------------------------------------
// App lifecycle
// ---------------------------------------------------------------------------

async function main(): Promise<void> {
  await app.whenReady();
  log.initialize();
  log.info('mando-electron starting');

  // Apply staged update from previous session (swap .app bundle + relaunch).
  // Must run before anything else -- if it triggers, the process exits.
  if (app.isPackaged && (await applyPendingUpdateIfAny())) return;

  const dataDir = getDataDir();

  // Start daemon (or discover running daemon).
  await ensureDaemon(dataDir);
  if (isQuitRequested()) return;
  await announceUiRegistered();

  await prepareRendererSource({
    mainBuildDir: __dirname,
    viteDevServerUrl: MAIN_WINDOW_VITE_DEV_SERVER_URL,
    viteName: MAIN_WINDOW_VITE_NAME,
  });
  setTrustedRendererOrigins(trustedRendererOrigins());
  installTrustedGatewayAuth();

  if (process.platform === 'darwin' && isHeadless()) {
    app.setActivationPolicy('accessory');
  }

  if (process.platform === 'darwin' && app.dock) {
    if (isHeadless()) {
      app.dock.hide();
    } else if (getAppMode() !== 'production') {
      const dockIcon = createDockIcon(resolveAsset('icon.png'), getAppMode());
      if (!dockIcon.isEmpty()) app.dock.setIcon(dockIcon);
    }
  }

  createMainWindow({ preloadDir: __dirname });
  if (!isHeadless()) {
    installTray(resolveAsset);
    registerShortcuts();
  }
  registerNotificationHandlers(() => getMainWindow());
  startHealthMonitor();

  setupAutoUpdate();

  // Login item is managed only via the Settings UI toggle (set-login-item IPC).
  runLoginItemMigration();

  app.on('activate', () => {
    if (isHeadless()) return;
    if (BrowserWindow.getAllWindows().length === 0) {
      createMainWindow({ preloadDir: __dirname });
    } else {
      showAndFocusMainWindow();
    }
  });
}

void main();

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});

// SIGTERM from `mando-dev stop` bypasses before-quit entirely.
app.on('before-quit', () => {
  runBeforeQuit({ stopRendererServer });
});
