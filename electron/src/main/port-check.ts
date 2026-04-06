import { app } from 'electron';
import path from 'path';
import fs from 'fs';
import { execSync } from 'child_process';
import log from '#main/logger';

/** Daemon launchd status. */
export interface DaemonStatus {
  loaded: boolean;
  running: boolean;
  pid: number | null;
}

function isDev(): boolean {
  return process.env.MANDO_APP_MODE === 'dev';
}

function isPreview(): boolean {
  return process.env.MANDO_APP_MODE === 'preview' || process.execPath.includes('Mando (Preview)');
}

function homeDir(): string {
  return app.getPath('home');
}

export function isServiceLoaded(label: string): boolean {
  try {
    execSync(`launchctl list ${label}`, { stdio: 'pipe' });
    return true;
  } catch {
    return false;
  }
}

function resolveDataDir(): string {
  if (process.env.MANDO_DATA_DIR) return process.env.MANDO_DATA_DIR;
  if (isPreview()) return path.join(homeDir(), '.mando-preview');
  if (isDev()) return path.join(homeDir(), '.mando-dev');
  return path.join(homeDir(), '.mando');
}

function resolvePortFileName(): string {
  if (isPreview()) return 'daemon.port';
  if (isDev()) return 'daemon-dev.port';
  return 'daemon.port';
}

/** Wait for a daemon port to become free (connection refused = free). */
export function waitForPortFree(deadline: number): void {
  const dataDir = resolveDataDir();
  const portFile = path.join(dataDir, resolvePortFileName());
  let port: number;
  try {
    port = parseInt(fs.readFileSync(portFile, 'utf-8').trim(), 10);
  } catch {
    return; // No port file — nothing to wait for
  }
  while (Date.now() < deadline) {
    try {
      // Try to connect — if refused, port is free
      execSync(`nc -z 127.0.0.1 ${port}`, { stdio: 'pipe', timeout: 1000 });
      // Connection succeeded — port still in use
      execSync('sleep 0.5', { stdio: 'pipe' });
    } catch {
      // Connection refused — port is free
      return;
    }
  }
  log.warn(`[launchd] port ${port} still in use after timeout`);
}

/** Poll until a launchd service is fully unloaded (or timeout). */
export function waitForServiceUnloaded(label: string, timeoutMs = 15000): void {
  const deadline = Date.now() + timeoutMs;
  // Phase 1: wait for launchd to report unloaded
  while (isServiceLoaded(label) && Date.now() < deadline) {
    execSync('sleep 0.2', { stdio: 'pipe' });
  }
  if (isServiceLoaded(label)) {
    log.warn(`[launchd] ${label} still loaded after timeout — proceeding`);
  }
  // Phase 2: wait for port to be free (only for daemon label)
  if (label.includes('daemon') && Date.now() < deadline) {
    waitForPortFree(deadline);
  }
}

function daemonLabel(): string {
  if (isPreview()) return 'build.mando.preview.daemon';
  return isDev() ? 'build.mando.daemon.dev' : 'build.mando.daemon';
}

/** Extract message string from an unknown error. */
function errorMsg(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** Get daemon status via launchctl. */
export function getDaemonStatus(): DaemonStatus {
  try {
    const out = execSync(`launchctl list ${daemonLabel()} 2>/dev/null`, { encoding: 'utf-8' });
    const pidMatch = out.match(/"PID"\s*=\s*(\d+)/);
    return {
      loaded: true,
      running: pidMatch !== null && pidMatch[1] !== '0',
      pid: pidMatch ? parseInt(pidMatch[1], 10) : null,
    };
  } catch (e: unknown) {
    // launchctl list exits non-zero when the service isn't loaded — that's expected.
    const msg = errorMsg(e);
    if (!msg.includes('Could not find service')) {
      log.warn('[launchd] daemon status check failed:', msg);
    }
    return { loaded: false, running: false, pid: null };
  }
}
