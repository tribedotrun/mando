/**
 * Mando Electron main process — thin client that talks to the daemon via HTTP.
 *
 * No napi loading. All data operations go through HTTP to the daemon.
 * Daemon owns runtime; Electron handles bootstrap, update, and login-item UX.
 */
import { app, BrowserWindow, Tray, Menu, globalShortcut, dialog, shell } from 'electron';
import path from 'path';
import fs from 'fs';
import type http from 'http';
import log from '#main/logger';
import { registerConfigHandlers } from '#main/config-handlers';
import { registerSetupValidationHandlers } from '#main/setup-validation';
import { getDevGitInfo } from '#main/dev-git-info';
import { installTrustedGatewayAuth } from '#main/gateway-auth';
import { handleTrusted, setTrustedRendererOrigins } from '#main/ipc-security';
import {
  getDataDir,
  getConfigPath,
  readPort,
  ensureDaemon,
  startHealthMonitor,
  cleanupDaemon,
  invalidateDiscoveryCache,
  setIsQuitting,
  updateTrayTooltip,
  getAppMode,
  getAppTitle,
  isHeadless,
} from '#main/daemon';
import { createTrayIcon, createDockIcon } from '#main/icons';
import { registerNotificationHandlers } from '#main/notifications';
import { registerTerminalBridgeHandlers } from '#main/terminal-bridge';
import { setupAutoUpdate, applyPendingUpdateIfAny, cleanupAutoUpdate } from '#main/updater';
import { getAppInfo } from '#main/app-info';
import { startRendererServer } from '#main/renderer-server';
import { announceUiQuitting, announceUiRegistered } from '#main/ui-lifecycle';

// Isolate Chromium user-data per data-dir so multiple dev/sandbox instances
// don't fight over ~/Library/Application Support/Electron/.
// Must run before app.whenReady() — Chromium locks the profile on startup.
const _dataDir = getDataDir();
if (getAppMode() !== 'production') {
  app.setPath('userData', path.join(_dataDir, 'electron-profile'));
}

let mainWindow: BrowserWindow | null = null;
let tray: Tray | null = null;
let trayAvailable = false;
let rendererServer: http.Server | null = null;
let rendererPort = 0;
let isQuitting = false;
let rendererUrl: string | null = null;

// ---------------------------------------------------------------------------
// Window management
// ---------------------------------------------------------------------------

function resolvePreload(): string {
  const candidates = [
    path.join(__dirname, '../preload/index.js'), // test-build layout
    path.join(__dirname, 'preload/index.js'), // forge .vite/build layout
  ];
  return candidates.find((p) => fs.existsSync(p)) || candidates[0];
}

function createWindow(): void {
  const headless = isHeadless();
  mainWindow = new BrowserWindow({
    show: false,
    width: headless ? 1800 : 1200,
    height: headless ? 1200 : 800,
    minWidth: 900,
    minHeight: 600,
    title: getAppTitle(),
    titleBarStyle: 'hidden',
    vibrancy: 'sidebar',
    trafficLightPosition: { x: 16, y: 16 },
    webPreferences: {
      preload: resolvePreload(),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      webSecurity: true,
    },
  });

  const url = rendererUrl ?? `http://127.0.0.1:${rendererPort}/index.html`;
  void mainWindow.loadURL(url);

  // Open external URLs in system browser; allow app-local (127.0.0.1) URLs
  const isAppLocal = (url: string) => {
    try {
      const u = new URL(url);
      return u.hostname === '127.0.0.1' || u.hostname === 'localhost';
    } catch {
      return false;
    }
  };

  const isWebUrl = (url: string) => url.startsWith('https://') || url.startsWith('http://');

  mainWindow.webContents.setWindowOpenHandler(({ url }) => {
    if (!isAppLocal(url)) {
      if (isWebUrl(url)) {
        void shell.openExternal(url);
      }
      return { action: 'deny' };
    }
    return { action: 'allow' };
  });

  mainWindow.webContents.on('will-navigate', (e, url) => {
    const currentUrl = mainWindow?.webContents.getURL() ?? '';
    if (url !== currentUrl && !isAppLocal(url)) {
      e.preventDefault();
      if (isWebUrl(url)) {
        void shell.openExternal(url);
      }
    }
  });

  // Show the window once content is ready (skip in headless mode).
  if (!headless) {
    mainWindow.once('ready-to-show', () => mainWindow?.show());
  }

  // Hide to tray on close (not quit). If the tray failed to create, fall
  // back to a normal close so the window is not orphaned with no way back.
  mainWindow.on('close', (e) => {
    if (!isQuitting && trayAvailable) {
      e.preventDefault();
      mainWindow?.hide();
    }
  });
}

function resolveAsset(name: string): string {
  const candidates = app.isPackaged
    ? [path.join(process.resourcesPath!, 'assets', name)]
    : [
        path.join(app.getAppPath(), 'assets', name),
        path.resolve(__dirname, '../../assets', name),
        path.resolve(__dirname, '../assets', name),
      ];
  return candidates.find((p) => fs.existsSync(p)) ?? candidates[0];
}

function createTray(): void {
  const icon = createTrayIcon(resolveAsset('trayTemplate@2x.png'), getAppMode());
  tray = new Tray(icon);
  tray.setToolTip(updateTrayTooltip());

  const title = getAppTitle();
  const contextMenu = Menu.buildFromTemplate([
    {
      label: `Show ${title}`,
      click: () => {
        mainWindow?.show();
        mainWindow?.focus();
      },
    },
    { type: 'separator' },
    {
      label: `Quit ${title}`,
      click: () => {
        isQuitting = true;
        setIsQuitting(true);
        app.quit();
      },
    },
  ]);

  tray.setContextMenu(contextMenu);
  trayAvailable = true;
}

function trustedRendererOrigins(): string[] {
  const origins = new Set<string>();
  if (rendererUrl) {
    origins.add(new URL(rendererUrl).origin);
  }
  if (rendererPort > 0) {
    origins.add(`http://127.0.0.1:${rendererPort}`);
    origins.add(`http://localhost:${rendererPort}`);
  }
  return Array.from(origins);
}

async function devServerReachable(url: string): Promise<boolean> {
  try {
    const response = await fetch(url, { method: 'HEAD' });
    return response.ok;
  } catch {
    return false;
  }
}

async function prepareRendererSource(): Promise<void> {
  if (MAIN_WINDOW_VITE_DEV_SERVER_URL) {
    if (await devServerReachable(MAIN_WINDOW_VITE_DEV_SERVER_URL)) {
      rendererUrl = MAIN_WINDOW_VITE_DEV_SERVER_URL;
      return;
    }

    log.warn(
      `[renderer] Vite dev server unavailable at ${MAIN_WINDOW_VITE_DEV_SERVER_URL}; falling back to static renderer`,
    );
  }

  const rendererDir = path.join(__dirname, `../renderer/${MAIN_WINDOW_VITE_NAME}`);
  const result = await startRendererServer(rendererDir);
  rendererPort = result.port;
  rendererServer = result.server;
  rendererUrl = `http://127.0.0.1:${rendererPort}/index.html`;
}

function registerShortcuts(): void {
  globalShortcut.register('CommandOrControl+N', () => {
    mainWindow?.webContents.send('shortcut', 'add-task');
  });
}

// ---------------------------------------------------------------------------
// IPC handlers — config operations via daemon HTTP
// ---------------------------------------------------------------------------

handleTrusted('get-gateway-url', async () => {
  const port =
    process.env.MANDO_GATEWAY_PORT ||
    (await readPort().catch((err: unknown) => {
      log.error('get-gateway-url: failed to read daemon port — daemon may not be running:', err);
      return null;
    }));
  if (!port) return null;
  return `http://127.0.0.1:${port}`;
});

handleTrusted('get-app-info', getAppInfo);

handleTrusted('restart-daemon', async () => {
  invalidateDiscoveryCache();
  return ensureDaemon(getDataDir());
});
handleTrusted('get-app-mode', () =>
  process.env.MANDO_HIDE_DEV_BAR === '1' ? 'clean' : getAppMode(),
);
handleTrusted('select-directory', async () => {
  const opts = { properties: ['openDirectory' as const], message: 'Select a project folder' };
  const win = BrowserWindow.getFocusedWindow();
  const result = win ? await dialog.showOpenDialog(win, opts) : await dialog.showOpenDialog(opts);
  return result.canceled ? null : (result.filePaths[0] ?? null);
});
handleTrusted('set-login-item', (_: unknown, enabled: boolean) => {
  if (app.isPackaged) {
    app.setLoginItemSettings({ openAtLogin: enabled, openAsHidden: true });
  }
});

handleTrusted('toggle-devtools', () => {
  mainWindow?.webContents.toggleDevTools();
});

handleTrusted('get-dev-git-info', getDevGitInfo);

// Setup validation handlers (Claude Code, Telegram) — see setup-validation.ts
registerSetupValidationHandlers();

// Config read/write, onboarding setup-complete, launchd — see config-handlers.ts
registerConfigHandlers();
registerTerminalBridgeHandlers();

// ---------------------------------------------------------------------------
// App lifecycle
// ---------------------------------------------------------------------------

void app.whenReady().then(async () => {
  log.initialize();
  log.info('mando-electron starting');

  // Apply staged update from previous session (swap .app bundle + relaunch).
  // Must run before anything else — if it triggers, the process exits.
  if (app.isPackaged && (await applyPendingUpdateIfAny())) return;

  const dataDir = getDataDir();

  // Start daemon (or discover running daemon).
  await ensureDaemon(dataDir);
  if (isQuitting) return;
  await announceUiRegistered();

  await prepareRendererSource();
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

  createWindow();
  if (!isHeadless()) {
    try {
      createTray();
    } catch (err) {
      // Tray creation can fail on missing icon assets or on systems without a
      // menu bar. Without a tray there is no way to reopen the app from a
      // hidden state, so the window close handler must fall back to quit.
      log.error('[main] tray creation failed, disabling close-to-tray:', err);
      trayAvailable = false;
    }
    registerShortcuts();
  }
  registerNotificationHandlers(() => mainWindow);
  startHealthMonitor();

  setupAutoUpdate();

  // Login item is managed only via the Settings UI toggle (set-login-item IPC).
  // MIGRATION-ONLY: move legacy top-level `startAtLogin` into `ui.openAtLogin`.
  // Keep this local to Electron startup so the daemon/Rust side stays free of
  // one-off config upgrade logic. Delete once old configs are no longer in use.
  if (app.isPackaged) {
    try {
      const raw = fs.readFileSync(getConfigPath(), 'utf-8');
      const cfg = JSON.parse(raw) as {
        startAtLogin?: boolean;
        ui?: { openAtLogin?: boolean };
      };
      if (cfg.startAtLogin !== undefined && cfg.ui?.openAtLogin === undefined) {
        const migrated = cfg.startAtLogin;
        app.setLoginItemSettings({ openAtLogin: migrated, openAsHidden: true });
        cfg.ui = { ...(cfg.ui || {}), openAtLogin: migrated };
        delete cfg.startAtLogin;
        fs.writeFileSync(getConfigPath(), JSON.stringify(cfg, null, 2), 'utf-8');
      }
    } catch (err: unknown) {
      const code = (err as NodeJS.ErrnoException)?.code;
      if (code === 'ENOENT') {
        // Fresh install, nothing to migrate.
      } else {
        log.error('[main] login-item config migration failed:', err);
      }
    }
  }

  app.on('activate', () => {
    if (isHeadless()) {
      return;
    }
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    } else {
      mainWindow?.show();
      mainWindow?.focus();
    }
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});

// In non-production mode, block external quit signals (e.g. Cmd+Q cascade from
// another Electron instance sharing the same bundle ID). Our tray menu sets
// isQuitting=true before calling app.quit(), so it still works. SIGTERM from
// `mando-dev stop` bypasses before-quit entirely.
app.on('before-quit', (e) => {
  if (!isQuitting && getAppMode() !== 'production') {
    e.preventDefault();
    return;
  }
  isQuitting = true;
  setIsQuitting(true);
  globalShortcut.unregisterAll();
  void announceUiQuitting();
  cleanupDaemon();
  cleanupAutoUpdate();
  rendererServer?.close();
});
