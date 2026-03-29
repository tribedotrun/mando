/**
 * Mando Electron main process — thin client that talks to the daemon via HTTP.
 *
 * No napi loading. All data operations go through HTTP to the daemon.
 * Daemon lifecycle managed via launchd (prod) or direct spawn (dev).
 */
import { app, BrowserWindow, Tray, Menu, globalShortcut, ipcMain, dialog, shell } from 'electron';
import path from 'path';
import fs from 'fs';
import { execSync } from 'child_process';
import http from 'http';
import log from '#main/logger';
import { installCliAndPlists, getDaemonStatus, removeLegacyAppPlist } from '#main/launchd';
import { registerSetupValidationHandlers } from '#main/setup-validation';
import {
  getDataDir,
  getConfigPath,
  readPort,
  readToken,
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
      sandbox: false,
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
      console.log(`[voice] Global shortcut registered: ${accel}`);
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

ipcMain.handle('get-gateway-url', async () => {
  const port =
    process.env.MANDO_GATEWAY_PORT ||
    (await readPort().catch((err: unknown) => {
      log.warn('get-gateway-url: failed to read daemon port, falling back to 18893:', err);
      return '18893';
    }));
  return `http://127.0.0.1:${port}`;
});

ipcMain.handle('get-auth-token', async () => {
  return (
    process.env.MANDO_AUTH_TOKEN ||
    (await readToken().catch((err: unknown) => {
      log.warn('get-auth-token: failed to read auth token, API calls will fail auth:', err);
      return '';
    }))
  );
});

ipcMain.handle('get-app-info', getAppInfo);

ipcMain.handle('get-data-dir', () => getDataDir());
ipcMain.handle('get-config-path', () => getConfigPath());
ipcMain.handle('get-connection-state', () => getConnectionState());
ipcMain.handle('get-app-mode', () => getAppMode());
ipcMain.handle('select-directory', async () => {
  const opts = { properties: ['openDirectory' as const], message: 'Select a project folder' };
  const win = BrowserWindow.getFocusedWindow();
  const result = win ? await dialog.showOpenDialog(win, opts) : await dialog.showOpenDialog(opts);
  return result.canceled ? null : (result.filePaths[0] ?? null);
});
ipcMain.handle('set-login-item', (_: unknown, enabled: boolean) => {
  if (app.isPackaged) {
    app.setLoginItemSettings({ openAtLogin: enabled, openAsHidden: true });
  }
});

ipcMain.handle('toggle-devtools', () => {
  mainWindow?.webContents.toggleDevTools();
});

ipcMain.handle('get-dev-git-info', () => {
  try {
    const branch = execSync('git rev-parse --abbrev-ref HEAD', { encoding: 'utf-8' }).trim();
    const toplevel = execSync('git rev-parse --show-toplevel', { encoding: 'utf-8' }).trim();
    const dirName = path.basename(toplevel);
    const parentName = path.basename(path.dirname(toplevel));
    const worktree = parentName === 'worktrees' ? dirName : null;
    const slotFile = path.join(toplevel, '.dev', 'slot');
    const slot = fs.existsSync(slotFile) ? fs.readFileSync(slotFile, 'utf-8').trim() : null;
    return { branch, worktree, slot };
  } catch (e: unknown) {
    log.debug('[get-dev-git-info] git info failed:', e);
    return { branch: 'unknown', worktree: null, slot: null };
  }
});

// Setup validation handlers (Claude Code, Telegram, Linear) — see setup-validation.ts
registerSetupValidationHandlers();

ipcMain.handle('has-config', async () => {
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

ipcMain.handle('read-config', async () => {
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

ipcMain.handle('save-config', async (_: unknown, configJson: string) => {
  try {
    const resp = await daemonFetch('/api/config', {
      method: 'PUT',
      body: configJson,
    });
    if (resp.ok) return true;
    const err = await resp.json().catch(() => ({}));
    log.error('save-config via daemon failed:', err);
  } catch (e: unknown) {
    log.error('save-config fetch failed:', e instanceof Error ? e.message : e);
  }
  // Fallback: write locally.
  log.warn('save-config: daemon unreachable, falling back to local file write');
  const configPath = getConfigPath();
  fs.mkdirSync(path.dirname(configPath), { recursive: true });
  fs.writeFileSync(configPath, configJson, 'utf-8');
  return true;
});

ipcMain.handle('setup-complete', async (_: unknown, configJson: string) => {
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
    log.error('setup-complete: failed to notify daemon after 5 attempts — captain may not start');
  }

  // Install CLI binary and launchd plists.
  try {
    installCliAndPlists(dataDir);
  } catch (e: unknown) {
    log.warn('launchd setup failed:', e instanceof Error ? e.message : e);
  }
  return true;
});

// -- Launchd IPC handlers (Electron-native) --
ipcMain.handle('launchd:reinstall', () => {
  installCliAndPlists(getDataDir());
  return true;
});
ipcMain.handle('launchd:daemon-status', () => getDaemonStatus());

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
    removeLegacyAppPlist();
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
