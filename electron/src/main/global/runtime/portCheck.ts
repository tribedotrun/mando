import path from 'path';
import fs from 'fs';
import { execSync } from 'child_process';
import log from '#main/global/providers/logger';
import { mustParsePortNumberText, parseLaunchctlPidText } from '#main/global/service/boundaryText';
import {
  daemonLabel,
  errorMsg,
  resolveDataDir,
  resolvePortFileName,
} from '#main/global/service/launchd';

/** Daemon launchd status. */
export interface DaemonStatus {
  loaded: boolean;
  running: boolean;
  pid: number | null;
}

export function isServiceLoaded(label: string): boolean {
  try {
    execSync(`launchctl list ${label}`, { stdio: 'pipe' });
    return true;
  } catch {
    return false;
  }
}

/** Wait for a daemon port to become free (connection refused = free). */
export function waitForPortFree(deadline: number): void {
  const dataDir = resolveDataDir();
  const portFile = path.join(dataDir, resolvePortFileName());
  let port: number;
  try {
    port = mustParsePortNumberText(fs.readFileSync(portFile, 'utf-8'), `file:${portFile}`);
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

/** Get daemon status via launchctl. */
export function getDaemonStatus(): DaemonStatus {
  try {
    const out = execSync(`launchctl list ${daemonLabel()} 2>/dev/null`, { encoding: 'utf-8' });
    const pid = parseLaunchctlPidText(out, `command:launchctl-list:${daemonLabel()}`);
    return {
      loaded: true,
      running: pid !== null,
      pid,
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
