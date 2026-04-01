/**
 * Mando Electron main process — thin client that talks to the daemon via HTTP.
 *
 * No napi loading. All data operations go through HTTP to the daemon.
 * Daemon lifecycle managed via launchd (prod) or direct spawn (dev).
 */
import { app, BrowserWindow, Tray, Menu, globalShortcut, dialog, shell } from 'electron';
import path from 'path';
import fs from 'fs';
import type http from 'http';
import log from '#main/logger';
import { installCliAndPlists, getDaemonStatus } from '#main/launchd';
import { registerSetupValidationHandlers } from '#main/setup-validation';
import { getDevGitInfo } from '#main/dev-git-info';
import { installTrustedGatewayAuth } from '#main/gateway-auth';
import { handleTrusted, setTrustedRendererOrigins } from '#main/ipc-security';
import {
  getDataDir,
  getConfigPath,
  readPort,
  daemonFetch,
  ensureDaemon,
  startHealthMonitor,
  cleanupDaemon,
  getConnectionState,
  setMainWindow,
  setIsQuitting,
  updateTrayTooltip,
  getAppMode,
  getAppTitle,
  isHeadless,
} from '#main/daemon';
import { createTrayIcon, createDockIcon } from '#main/icons';
import { registerNotificationHandlers } from '#main/notifications';
import { setupAutoUpdate, applyPendingUpdateIfAny } from '#main/updater';
import { createVoiceWindow, onVoiceHotkeyDown } from '#main/voice-window';
import { getAppInfo } from '#main/app-info';
import { startRendererServer } from '#main/renderer-server';

let mainWindow: BrowserWindow | null = null;
let tray: Tray | null = null;
let rendererServer: http.Server | null = null;
let rendererPort = 0;
let isQuitting = false;

class DaemonConfigHttpError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'DaemonConfigHttpError';
  }
}

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

  if (MAIN_WINDOW_VITE_DEV_SERVER_URL) {
    mainWindow.loadURL(MAIN_WINDOW_VITE_DEV_SERVER_URL);
  } else {
    mainWindow.loadURL(`http://127.0.0.1:${rendererPort}/index.html`);
  }

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
        shell.openExternal(url);
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
        shell.openExternal(url);
      }
    }
  });

  // Show the window once content is ready (skip in headless mode).
  if (!headless) {
    mainWindow.once('ready-to-show', () => mainWindow?.show());
  }

  // Hide to tray on close (not quit)
  mainWindow.on('close', (e) => {
    if (!isQuitting) {
      e.preventDefault();
      mainWindow?.hide();
    }
  });

  setMainWindow(mainWindow);
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
    {
      label: 'Voice Input',
      accelerator: 'Alt+Space',
      click: () => {
        onVoiceHotkeyDown(resolvePreload, voiceRendererUrl());
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
}

function voiceRendererUrl(): string {
  return MAIN_WINDOW_VITE_DEV_SERVER_URL
    ? `${MAIN_WINDOW_VITE_DEV_SERVER_URL}?voice=1`
    : `http://127.0.0.1:${rendererPort}/index.html?voice=1`;
}

function trustedRendererOrigins(): string[] {
  const origins = new Set<string>();
  if (MAIN_WINDOW_VITE_DEV_SERVER_URL) {
    origins.add(new URL(MAIN_WINDOW_VITE_DEV_SERVER_URL).origin);
  }
  if (rendererPort > 0) {
    origins.add(`http://127.0.0.1:${rendererPort}`);
    origins.add(`http://localhost:${rendererPort}`);
  }
  return Array.from(origins);
}

function registerShortcuts(): void {
  globalShortcut.register('CommandOrControl+N', () => {
    mainWindow?.webContents.send('shortcut', 'add-task');
  });

  // Register voice shortcut — try multiple accelerators since Alt+Space
  // is often claimed by macOS input source switching.
  const voiceAccelerators = [
    'Alt+Space',
    'CommandOrControl+Shift+Space',
    'CommandOrControl+Shift+V',
  ];
  let voiceRegistered = false;
  for (const accel of voiceAccelerators) {
    if (
      globalShortcut.register(accel, () => {
        onVoiceHotkeyDown(resolvePreload, voiceRendererUrl());
      })
    ) {
      log.info(`[voice] Global shortcut registered: ${accel}`);
      voiceRegistered = true;
      break;
    }
    log.debug(`[voice] Failed to register: ${accel}, trying next...`);
  }
  if (!voiceRegistered) {
    log.warn('[voice] All shortcut accelerators failed — voice window only available via menu bar');
  }
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

handleTrusted('get-data-dir', () => getDataDir());
handleTrusted('get-config-path', () => getConfigPath());
handleTrusted('get-connection-state', () => getConnectionState());
handleTrusted('get-app-mode', () => getAppMode());
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

// Setup validation handlers (Claude Code, Telegram, Linear) — see setup-validation.ts
registerSetupValidationHandlers();

handleTrusted('has-config', async () => {
  try {
    const resp = await daemonFetch('/api/config/status');
    if (resp.ok) {
      const data = (await resp.json()) as { setupComplete: boolean };
      return data.setupComplete;
    }
  } catch (e: unknown) {
    log.debug('[has-config] daemon check failed:', e);
  }
  const configPath = getConfigPath();
  if (!fs.existsSync(configPath)) return false;
  try {
    const raw = fs.readFileSync(configPath, 'utf-8');
    return typeof JSON.parse(raw) === 'object';
  } catch (e: unknown) {
    log.warn('[has-config] local config parse failed:', e);
    return false;
  }
});

handleTrusted('read-config', async () => {
  try {
    const resp = await daemonFetch('/api/config');
    if (resp.ok) return await resp.text();
  } catch (err: unknown) {
    log.debug('read-config: daemon not ready, falling back to local file:', err);
  }
  try {
    return fs.readFileSync(getConfigPath(), 'utf-8');
  } catch (err: unknown) {
    log.error('read-config: both daemon and local file read failed:', err);
    throw new Error('Failed to load config from daemon and local file', { cause: err });
  }
});

handleTrusted('save-config', async (_: unknown, configJson: string) => {
  try {
    const resp = await daemonFetch('/api/config', {
      method: 'PUT',
      body: configJson,
    });
    if (resp.ok) return true;
    const err = await resp.json().catch(() => ({ error: resp.statusText }));
    log.error('save-config via daemon failed:', err);
    throw new DaemonConfigHttpError(err.error || `HTTP ${resp.status}`);
  } catch (e: unknown) {
    if (e instanceof DaemonConfigHttpError) {
      throw e;
    }

    const message = e instanceof Error ? e.message : String(e);
    const networkFallback =
      message.includes('fetch failed') ||
      message.includes('ECONNREFUSED') ||
      message.includes('ENOTFOUND') ||
      message.includes('timed out');

    if (!networkFallback) {
      throw e;
    }

    log.error('save-config fetch failed:', message);
  }
  // Fallback: write locally so config isn't lost.
  log.warn('save-config: daemon unreachable, falling back to local file write');
  const configPath = getConfigPath();
  fs.mkdirSync(path.dirname(configPath), { recursive: true });
  fs.writeFileSync(configPath, configJson, 'utf-8');
  return true;
});

handleTrusted('setup-complete', async (_: unknown, configJson: string) => {
  const dataDir = getDataDir();

  // Create directory structure (needed before daemon can start).
  for (const sub of ['state', 'logs', 'images']) {
    fs.mkdirSync(path.join(dataDir, sub), { recursive: true });
  }
  const configPath = getConfigPath();
  fs.mkdirSync(path.dirname(configPath), { recursive: true });
  fs.writeFileSync(configPath, configJson, 'utf-8');
  // Start daemon (config is on disk now).
  await ensureDaemon(dataDir);

  // Notify daemon about setup completion (retry — daemon may still be starting).
  let setupNotified = false;
  for (let attempt = 0; attempt < 5; attempt++) {
    try {
      await daemonFetch('/api/config/setup', {
        method: 'POST',
        body: JSON.stringify({
          config: JSON.parse(configJson),
        }),
      });
      setupNotified = true;
      break;
    } catch (err: unknown) {
      log.debug(`setup-complete: attempt ${attempt + 1}/5 failed:`, err);
      await new Promise((r) => setTimeout(r, 1000 * (attempt + 1)));
    }
  }
  if (!setupNotified) {
    log.error(
      'setup-complete: failed to notify daemon after 5 attempts — daemon will pick up config on next restart, but captain may not start until then',
    );
  }

  // Install CLI binary and launchd plists (skip in sandbox — sandbox manages its own processes).
  if (getAppMode() !== 'sandbox') {
    try {
      installCliAndPlists(dataDir);
    } catch (e: unknown) {
      log.warn('launchd setup failed:', e instanceof Error ? e.message : e);
    }
  }
  return true;
});

// -- Launchd IPC handlers (Electron-native) --
handleTrusted('launchd:reinstall', () => {
  installCliAndPlists(getDataDir());
  return true;
});
handleTrusted('launchd:daemon-status', () => getDaemonStatus());
handleTrusted('open-logs-folder', () => shell.openPath(path.join(getDataDir(), 'logs')));

// ---------------------------------------------------------------------------
// App lifecycle
// ---------------------------------------------------------------------------

app.whenReady().then(async () => {
  log.initialize();
  log.info('mando-electron starting');

  // Apply staged update from previous session (swap .app bundle + relaunch).
  // Must run before anything else — if it triggers, the process exits.
  if (app.isPackaged && applyPendingUpdateIfAny()) return;

  const dataDir = getDataDir();

  // Start daemon (or discover running daemon).
  await ensureDaemon(dataDir);
  if (isQuitting) return;

  if (!MAIN_WINDOW_VITE_DEV_SERVER_URL) {
    const rendererDir = path.join(__dirname, `../renderer/${MAIN_WINDOW_VITE_NAME}`);
    const result = await startRendererServer(rendererDir);
    rendererPort = result.port;
    rendererServer = result.server;
  }
  setTrustedRendererOrigins(trustedRendererOrigins());
  installTrustedGatewayAuth();

  if (process.platform === 'darwin' && isHeadless()) {
    app.setActivationPolicy('accessory');
  }

  if (process.platform === 'darwin' && app.dock) {
    if (isHeadless()) {
      app.dock.hide();
    } else {
      const dockIcon = createDockIcon(resolveAsset('icon.png'), getAppMode());
      if (!dockIcon.isEmpty()) app.dock.setIcon(dockIcon);
    }
  }

  createWindow();
  if (!isHeadless()) {
    try {
      createTray();
    } catch (err) {
      log.warn('[main] tray creation failed:', err);
    }
    registerShortcuts();
  }
  // Voice window after main so CDP tools connect to main window first.
  createVoiceWindow(resolvePreload, voiceRendererUrl());
  registerNotificationHandlers(() => mainWindow);
  startHealthMonitor();

  setupAutoUpdate();

  if (app.isPackaged) {
    let openAtLogin = true;
    try {
      const raw = fs.readFileSync(getConfigPath(), 'utf-8');
      const cfg = JSON.parse(raw) as { startAtLogin?: boolean };
      if (cfg.startAtLogin === false) openAtLogin = false;
    } catch (err: unknown) {
      const code = (err as NodeJS.ErrnoException).code;
      if (code !== 'ENOENT') {
        log.warn('[login-item] failed to read config, defaulting to enabled:', err);
      }
    }
    app.setLoginItemSettings({ openAtLogin, openAsHidden: true });
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

app.on('before-quit', async () => {
  isQuitting = true;
  setIsQuitting(true);
  globalShortcut.unregisterAll();
  cleanupDaemon();
  rendererServer?.close();
});
