/**
 * Daemon connection management — discovery, health checks, reconnection,
 * version handshake, and HTTP fetch helper.
 */
import { app, BrowserWindow, dialog } from 'electron';
import path from 'path';
import fs from 'fs';
import os from 'os';
import log from '#main/logger';
import { readAppPackageVersion } from '#main/app-package';
import {
  stageDaemonBinary,
  installDaemonPlist,
  updateDaemonBinary,
  rollbackDaemonBinary,
  kickstartDaemon,
  bootoutDevServices,
} from '#main/launchd';

// -- Connection state --
export type ConnectionState = 'connecting' | 'connected' | 'disconnected' | 'updating';
let connectionState: ConnectionState = 'connecting';
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let reconnectDelay = 1000;
let reconnectAttempts = 0;
const MAX_RECONNECT_DELAY = 30000;
const KICKSTART_AFTER_ATTEMPTS = 3;
let isQuittingRef = false;

// -- Daemon discovery cache --
let cachedPort: string | null = null;
let cachedToken: string | null = null;

// -- Window reference for sending IPC messages --
let mainWindowRef: BrowserWindow | null = null;

export function setMainWindow(win: BrowserWindow | null): void {
  mainWindowRef = win;
}

export function setIsQuitting(quitting: boolean): void {
  isQuittingRef = quitting;
}

export function getConnectionState(): ConnectionState {
  return connectionState;
}

// ---------------------------------------------------------------------------
// Data directory and config path helpers
// ---------------------------------------------------------------------------

export function getDataDir(): string {
  if (process.env.MANDO_DATA_DIR) return process.env.MANDO_DATA_DIR;
  // Dev mode uses an isolated data directory to avoid conflicts with prod.
  if (getAppMode() === 'dev') return path.join(os.homedir(), '.mando-dev');
  return path.join(os.homedir(), '.mando');
}

export function getConfigPath(): string {
  return process.env.MANDO_CONFIG || path.join(getDataDir(), 'config.json');
}

// ---------------------------------------------------------------------------
// App mode: production | dev | sandbox
// ---------------------------------------------------------------------------

export type AppMode = 'production' | 'dev' | 'prod-local' | 'sandbox';

export function getAppMode(): AppMode {
  const mode = process.env.MANDO_APP_MODE;
  if (mode === 'dev' || mode === 'sandbox' || mode === 'prod-local') return mode;
  return 'production';
}

export function isHeadless(): boolean {
  if (process.env.MANDO_HEADLESS === '1') return true;
  return getAppMode() === 'sandbox' && process.env.MANDO_SANDBOX_VISIBLE !== '1';
}

// ---------------------------------------------------------------------------
// Daemon discovery: port + auth token
// ---------------------------------------------------------------------------

export async function readPort(): Promise<string> {
  if (cachedPort) return cachedPort;
  const dataDir = getDataDir();
  const portFileName = getAppMode() === 'dev' ? 'daemon-dev.port' : 'daemon.port';
  const portFile = path.join(dataDir, portFileName);
  const content = await fs.promises.readFile(portFile, 'utf-8');
  cachedPort = content.trim();
  return cachedPort;
}

export async function readToken(): Promise<string> {
  if (cachedToken) return cachedToken;
  const dataDir = getDataDir();
  const tokenFile = path.join(dataDir, 'auth-token');
  const content = await fs.promises.readFile(tokenFile, 'utf-8');
  cachedToken = content.trim();
  return cachedToken;
}

async function hasExternalGatewayToken(dataDir: string): Promise<boolean> {
  const envToken = process.env.MANDO_AUTH_TOKEN?.trim();
  if (envToken) return true;

  try {
    const tokenFile = path.join(dataDir, 'auth-token');
    const content = await fs.promises.readFile(tokenFile, 'utf-8');
    return content.trim().length > 0;
  } catch {
    return false;
  }
}

/** Invalidate cached port/token (e.g., after daemon restart). */
export function invalidateDiscoveryCache(): void {
  cachedPort = null;
  cachedToken = null;
}

/** Fetch from daemon with auth token. */
export async function daemonFetch(urlPath: string, options?: RequestInit): Promise<Response> {
  const port = process.env.MANDO_GATEWAY_PORT || (await readPort());
  const token = process.env.MANDO_AUTH_TOKEN || (await readToken());
  const url = `http://127.0.0.1:${port}${urlPath}`;
  const headers: Record<string, string> = {
    ...(options?.headers as Record<string, string>),
  };
  if (token) headers['Authorization'] = `Bearer ${token}`;
  if (!headers['Content-Type'] && options?.body) {
    headers['Content-Type'] = 'application/json';
  }
  return fetch(url, { ...options, headers });
}

function isProcessAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (err: unknown) {
    // EPERM = process exists but we lack permission to signal it — still alive.
    if ((err as NodeJS.ErrnoException).code === 'EPERM') return true;
    return false;
  }
}

// ---------------------------------------------------------------------------
// Connection state machine
// ---------------------------------------------------------------------------

function setConnectionState(state: ConnectionState): void {
  connectionState = state;
  mainWindowRef?.webContents.send('connection-state', state);
}

export function getAppTitle(): string {
  const mode = getAppMode();
  if (mode === 'dev') return 'Mando (Dev)';
  if (mode === 'prod-local') return 'Mando (Prod Local)';
  if (mode === 'sandbox') return 'Mando (Sandbox)';
  return 'Mando';
}

export function updateTrayTooltip(): string {
  const title = getAppTitle();
  const tooltips: Record<ConnectionState, string> = {
    connecting: `${title} — Connecting...`,
    connected: `${title} — Connected`,
    disconnected: `${title} — Disconnected`,
    updating: `${title} — Updating daemon...`,
  };
  return tooltips[connectionState];
}

async function healthCheck(): Promise<{
  healthy: boolean;
  version?: string;
}> {
  try {
    const port = process.env.MANDO_GATEWAY_PORT || (await readPort());
    const url = `http://127.0.0.1:${port}/api/health`;
    const resp = await fetch(url, { signal: AbortSignal.timeout(5000) });
    if (resp.ok) {
      return (await resp.json()) as { healthy: boolean; version?: string };
    }
    log.debug(`healthCheck: HTTP ${resp.status} from daemon`);
    return { healthy: false };
  } catch (err: unknown) {
    log.debug('healthCheck failed:', err instanceof Error ? err.message : err);
    return { healthy: false };
  }
}

async function waitForDaemon(timeoutMs = 15000): Promise<boolean> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const result = await healthCheck();
    if (result.healthy) {
      setConnectionState('connected');
      reconnectDelay = 1000;
      reconnectAttempts = 0;
      return true;
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  return false;
}

function scheduleReconnect(): void {
  if (reconnectTimer) return;
  if (isQuittingRef) return;

  reconnectTimer = setTimeout(async () => {
    reconnectTimer = null;
    invalidateDiscoveryCache();

    const result = await healthCheck();
    if (result.healthy) {
      setConnectionState('connected');
      reconnectDelay = 1000;
      reconnectAttempts = 0;
      return;
    }

    reconnectAttempts++;

    // After several failed reconnects, the daemon may be stuck (launchd
    // throttling a crash-loop, or service loaded but not running).
    // Kickstart tells launchd to start it immediately, bypassing throttle.
    if (reconnectAttempts === KICKSTART_AFTER_ATTEMPTS) {
      log.info('[daemon] reconnect attempts exhausted — kickstarting via launchd');
      kickstartDaemon();
    }

    reconnectDelay = Math.min(reconnectDelay * 2, MAX_RECONNECT_DELAY);
    setConnectionState('disconnected');
    scheduleReconnect();
  }, reconnectDelay);
}

export function startHealthMonitor(): void {
  setInterval(async () => {
    if (isQuittingRef) return;
    if (connectionState === 'updating') return;

    const result = await healthCheck();
    if (result.healthy && connectionState !== 'connected') {
      // Daemon came back — invalidate cached port/token in case it restarted on a different port.
      invalidateDiscoveryCache();
      setConnectionState('connected');
      reconnectDelay = 1000;
      reconnectAttempts = 0;
    } else if (!result.healthy && connectionState === 'connected') {
      // Daemon went away — invalidate so next reconnect discovers the new port.
      invalidateDiscoveryCache();
      setConnectionState('disconnected');
      scheduleReconnect();
    }
  }, 10000);
}

// ---------------------------------------------------------------------------
// Version handshake + update cycle
// ---------------------------------------------------------------------------

async function checkVersionAndUpdate(dataDir: string): Promise<void> {
  const result = await healthCheck();
  if (!result.healthy || !result.version) return;

  const bundledVersion = readAppPackageVersion();
  if (!bundledVersion || result.version === bundledVersion) return;

  log.info(`Version mismatch: daemon=${result.version}, bundled=${bundledVersion}. Updating...`);
  setConnectionState('updating');

  const success = updateDaemonBinary(dataDir);
  if (!success) {
    log.error('Daemon binary update failed');
    setConnectionState('disconnected');
    return;
  }

  invalidateDiscoveryCache();
  const ready = await waitForDaemon(10000);
  if (!ready) {
    log.error('Updated daemon failed health check, rolling back');
    rollbackDaemonBinary(dataDir);
    invalidateDiscoveryCache();
    await waitForDaemon(10000);
  }
}

// ---------------------------------------------------------------------------
// Daemon startup
// ---------------------------------------------------------------------------

/** Kill a daemon by PID (SIGTERM → wait → SIGKILL) and clean up stale files. */
async function killDaemonByPid(pid: number, dataDir: string): Promise<boolean> {
  try {
    process.kill(pid, 'SIGTERM');
  } catch (err: unknown) {
    const code = (err as NodeJS.ErrnoException).code;
    if (code === 'ESRCH') return true; // already dead
    log.error(`Failed to SIGTERM daemon pid ${pid}: ${code ?? err}`);
    return false;
  }

  // Wait up to 3s for graceful exit.
  for (let i = 0; i < 12; i++) {
    try {
      process.kill(pid, 0);
    } catch (err: unknown) {
      if ((err as NodeJS.ErrnoException).code === 'ESRCH') break; // dead
      log.warn(`[daemon] unexpected error checking pid ${pid}:`, err);
      break;
    }
    await new Promise((r) => setTimeout(r, 250));
  }

  // Force-kill if still alive.
  try {
    process.kill(pid, 0);
    process.kill(pid, 'SIGKILL');
    await new Promise((r) => setTimeout(r, 500));
  } catch {
    // Expected: ESRCH if already dead
  }

  // Verify it's actually dead.
  if (isProcessAlive(pid)) {
    log.error(`Daemon pid ${pid} survived SIGKILL`);
    return false;
  }

  // Clean up files the daemon may not have cleaned after SIGKILL.
  for (const f of ['daemon.pid', 'daemon.port', 'daemon-dev.port']) {
    try {
      fs.unlinkSync(path.join(dataDir, f));
    } catch {
      /* ok */
    }
  }
  return true;
}

export async function ensureDaemon(dataDir: string): Promise<boolean> {
  // MANDO_EXTERNAL_GATEWAY: skip daemon management entirely (for testing).
  if (process.env.MANDO_EXTERNAL_GATEWAY) {
    if (!(await hasExternalGatewayToken(dataDir))) {
      setConnectionState('disconnected');
      scheduleReconnect();
      return false;
    }

    const ready = await waitForDaemon(10000);
    if (!ready) {
      setConnectionState('disconnected');
      scheduleReconnect();
    }
    return ready;
  }

  // Check if daemon is already running.
  const health = await healthCheck();
  if (health.healthy) {
    setConnectionState('connected');
    await checkVersionAndUpdate(dataDir);
    return true;
  }

  // Check for another daemon via PID file.
  const pidFile = path.join(dataDir, 'daemon.pid');
  try {
    const pid = parseInt(fs.readFileSync(pidFile, 'utf-8').trim(), 10);
    if (!isNaN(pid) && isProcessAlive(pid)) {
      log.info(`Killing existing daemon (pid ${pid}) before starting fresh...`);
      const killed = await killDaemonByPid(pid, dataDir);
      invalidateDiscoveryCache();
      if (!killed) {
        dialog.showErrorBox(
          'Mando — Cannot Start',
          `Could not stop the existing daemon (PID ${pid}).\n\nKill it manually: kill -9 ${pid}`,
        );
        app.quit();
        return false;
      }
    }
  } catch (err: unknown) {
    const code = (err as NodeJS.ErrnoException).code;
    if (code !== 'ENOENT') {
      log.warn(`Could not read PID file ${pidFile}: ${err}`);
    }
  }

  // Try to start daemon via launchd (both dev and prod).
  if (!fs.existsSync(path.join(dataDir, 'config.json'))) {
    // No config yet — daemon can't start. Will start after onboarding.
    return false;
  }
  stageDaemonBinary();
  installDaemonPlist(dataDir);

  const ready = await waitForDaemon(15000);
  if (!ready) {
    setConnectionState('disconnected');
    scheduleReconnect();
  }
  return ready;
}

/** Clean up reconnect timer and stop dev daemon. */
export function cleanupDaemon(): void {
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
  // In dev, stop the launchd-managed daemon on quit.
  // In prod, the daemon persists across Electron restarts via KeepAlive.
  bootoutDevServices();
}
