/**
 * Owner for the main BrowserWindow. Single authority for creating,
 * showing, hiding, and closing the window. Replaces the ambient
 * `mainWindow` module-level binding.
 *
 * Codifies invariants M1 and M2 in .claude/skills/s-arch/invariants.md.
 */
import { BrowserWindow, shell } from 'electron';
import { getAppMode, isHeadless } from '#main/global/config/lifecycle';
import { getAppTitle } from '#main/global/service/lifecycle';
import { resolvePreload } from '#main/global/service/preloadResolver';
import { isQuitRequested } from '#main/global/runtime/quitController';

interface WindowRuntime {
  mainWindow: BrowserWindow | null;
  rendererUrl: string | null;
  rendererPort: number;
  trayAvailable: boolean;
}

const runtime: WindowRuntime = {
  mainWindow: null,
  rendererUrl: null,
  rendererPort: 0,
  trayAvailable: false,
};

export function getMainWindow(): BrowserWindow | null {
  return runtime.mainWindow;
}

export function getRendererUrl(): string | null {
  return runtime.rendererUrl;
}

export function getRendererPort(): number {
  return runtime.rendererPort;
}

export function setRenderer(url: string, port: number): void {
  runtime.rendererUrl = url;
  runtime.rendererPort = port;
}

export function setTrayAvailable(available: boolean): void {
  runtime.trayAvailable = available;
}

export function isTrayAvailable(): boolean {
  return runtime.trayAvailable;
}

function isAppLocal(url: string): boolean {
  try {
    const origin = new URL(url).origin;
    return trustedRendererOrigins().includes(origin);
  } catch {
    return false;
  }
}

function isWebUrl(url: string): boolean {
  return url.startsWith('https://') || url.startsWith('http://');
}

export function createMainWindow(opts: { preloadDir: string }): BrowserWindow {
  const headless = isHeadless();
  const window = new BrowserWindow({
    show: false,
    width: headless ? 1800 : 1200,
    height: headless ? 1200 : 800,
    minWidth: 900,
    minHeight: 600,
    title: getAppTitle(getAppMode()),
    titleBarStyle: 'hidden',
    vibrancy: 'sidebar',
    trafficLightPosition: { x: 16, y: 16 },
    webPreferences: {
      preload: resolvePreload(opts.preloadDir),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      webSecurity: true,
    },
  });

  const url = runtime.rendererUrl ?? `http://127.0.0.1:${runtime.rendererPort}/index.html`;
  void window.loadURL(url);

  window.webContents.setWindowOpenHandler(({ url }) => {
    if (!isAppLocal(url)) {
      if (isWebUrl(url)) {
        void shell.openExternal(url);
      }
      return { action: 'deny' };
    }
    return { action: 'allow' };
  });

  window.webContents.on('will-navigate', (e, url) => {
    const currentUrl = window.webContents.getURL() ?? '';
    if (url !== currentUrl && !isAppLocal(url)) {
      e.preventDefault();
      if (isWebUrl(url)) {
        void shell.openExternal(url);
      }
    }
  });

  // Show the window once content is ready (skip in headless mode).
  if (!headless) {
    window.once('ready-to-show', () => window.show());
  }

  // Hide to tray on close (not quit). If the tray failed to create, fall
  // back to a normal close so the window is not orphaned with no way back.
  window.on('close', (e) => {
    if (!isQuitRequested() && runtime.trayAvailable) {
      e.preventDefault();
      window.hide();
    }
  });

  runtime.mainWindow = window;
  return window;
}

export function showAndFocusMainWindow(): void {
  runtime.mainWindow?.show();
  runtime.mainWindow?.focus();
}

export function trustedRendererOrigins(): string[] {
  const origins = new Set<string>();
  if (runtime.rendererUrl) {
    origins.add(new URL(runtime.rendererUrl).origin);
  }
  if (runtime.rendererPort > 0) {
    origins.add(`http://127.0.0.1:${runtime.rendererPort}`);
    origins.add(`http://localhost:${runtime.rendererPort}`);
  }
  return Array.from(origins);
}
