/**
 * Daemon connection management — discovery, health checks, reconnection,
 * version handshake, and HTTP fetch helper.
 */
import { app, dialog } from 'electron';
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
let healthMonitorInterval: ReturnType<typeof setInterval> | null = null;
let reconnectDelay = 1000;
let reconnectAttempts = 0;
const MAX_RECONNECT_DELAY = 30000;
const KICKSTART_AFTER_ATTEMPTS = 3;
let isQuittingRef = false;

// -- Daemon discovery cache --
let cachedPort: string | null = null;
let cachedToken: string | null = null;

export function setIsQuitting(quitting: boolean): void {
  isQuittingRef = quitting;
}

// ---------------------------------------------------------------------------
// Data directory and config path helpers
// ---------------------------------------------------------------------------

export function getDataDir(): string {
  if (process.env.MANDO_DATA_DIR) return process.env.MANDO_DATA_DIR;
  // Dev and preview modes use isolated data directories to avoid conflicts with prod.
  if (getAppMode() === 'preview') return path.join(os.homedir(), '.mando-preview');
  if (getAppMode() === 'dev') return path.join(os.homedir(), '.mando-dev');
  return path.join(os.homedir(), '.mando');
}

export function getConfigPath(): string {
  return process.env.MANDO_CONFIG || path.join(getDataDir(), 'config.json');
}

// ---------------------------------------------------------------------------
// App mode: production | dev | sandbox
// ---------------------------------------------------------------------------

export type AppMode = 'production' | 'dev' | 'prod-local' | 'sandbox' | 'preview';

export function getAppMode(): AppMode {
  const mode = process.env.MANDO_APP_MODE;
  if (mode === 'dev' || mode === 'sandbox' || mode === 'prod-local' || mode === 'preview')
    return mode;
  // Detect preview mode from app bundle path when launched from /Applications.
  if (app.isPackaged && process.execPath.includes('Mando (Preview)')) return 'preview';
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
  const mode = getAppMode();
  const portFileName = mode === 'dev' ? 'daemon-dev.port' : 'daemon.port';
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
  } catch (err: unknown) {
    const code = (err as NodeJS.ErrnoException)?.code;
    if (code !== 'ENOENT') {
      log.warn('[daemon] hasExternalGatewayToken: unexpected error reading token file:', err);
    }
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
}

export function getAppTitle(): string {
  const mode = getAppMode();
  if (mode === 'dev') return 'Mando (Dev)';
  if (mode === 'preview') return 'Mando (Preview)';
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

let healthCheckFailureStreak = 0;

async function healthCheck(): Promise<{
  healthy: boolean;
  version?: string;
}> {
  try {
    const port = process.env.MANDO_GATEWAY_PORT || (await readPort());
    const url = `http://127.0.0.1:${port}/api/health`;
    const resp = await fetch(url, { signal: AbortSignal.timeout(5000) });
    if (resp.ok) {
      healthCheckFailureStreak = 0;
      return (await resp.json()) as { healthy: boolean; version?: string };
    }
    // First failure in a streak: log at info so forensic investigation is
    // possible without enabling debug logging. Subsequent failures drop to
    // debug to avoid filling the log with a stuck daemon's HTTP errors.
    if (healthCheckFailureStreak === 0) {
      log.info(`healthCheck: HTTP ${resp.status} from daemon`);
    } else {
      log.debug(`healthCheck: HTTP ${resp.status} from daemon`);
    }
    healthCheckFailureStreak++;
    return { healthy: false };
  } catch (err: unknown) {
    const reason = err instanceof Error ? err.message : String(err);
    if (healthCheckFailureStreak === 0) {
      log.info(`healthCheck failed: ${reason}`);
    } else {
      log.debug(`healthCheck failed: ${reason}`);
    }
    healthCheckFailureStreak++;
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
  healthMonitorInterval = setInterval(async () => {
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

/** Compare two semver strings. Returns >0 if a > b, 0 if equal, <0 if a < b. */
function compareSemver(a: string, b: string): number {
  const pa = a.split('.').map(Number);
  const pb = b.split('.').map(Number);
  for (let i = 0; i < 3; i++) {
    const diff = (pa[i] || 0) - (pb[i] || 0);
    if (diff !== 0) return diff;
  }
  return 0;
}

async function checkVersionAndUpdate(dataDir: string): Promise<void> {
  const result = await healthCheck();
  if (!result.healthy || !result.version) return;

  const bundledVersion = readAppPackageVersion();
  if (!bundledVersion || compareSemver(bundledVersion, result.version) <= 0) {
    if (bundledVersion && compareSemver(bundledVersion, result.version) < 0) {
      log.info(
        `Daemon ${result.version} is newer than bundled ${bundledVersion} — skipping update`,
      );
    }
    return;
  }

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
  } catch (err: unknown) {
    // ESRCH means the process is already dead, which is what we want.
    // Log any other error (e.g. EPERM) so we don't silently hide failures.
    if ((err as NodeJS.ErrnoException | null)?.code !== 'ESRCH') {
      log.warn(`[daemon] force-kill pid ${pid} unexpected error:`, err);
    }
  }

  // Verify it's actually dead.
  if (isProcessAlive(pid)) {
    log.error(`Daemon pid ${pid} survived SIGKILL`);
    return false;
  }

  // Clean up files the daemon may not have cleaned after SIGKILL.
  // ENOENT is expected (files may not exist); other errors are logged
  // so permission/disk issues don't pass silently.
  for (const f of ['daemon.pid', 'daemon.port', 'daemon-dev.port', 'daemon-preview.port']) {
    try {
      fs.unlinkSync(path.join(dataDir, f));
    } catch (err: unknown) {
      if ((err as NodeJS.ErrnoException)?.code !== 'ENOENT') {
        log.warn(`[daemon] cleanup of ${f} failed:`, err);
      }
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

/** Clean up all timers and stop dev daemon. */
export function cleanupDaemon(): void {
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
  if (healthMonitorInterval) {
    clearInterval(healthMonitorInterval);
    healthMonitorInterval = null;
  }
  // In dev, stop the launchd-managed daemon on quit.
  // In prod, the daemon persists across Electron restarts via KeepAlive.
  bootoutDevServices();
}
