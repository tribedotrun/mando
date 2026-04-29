import path from 'path';
import fs from 'fs';
import { execFile, execSync } from 'child_process';
import { promisify } from 'util';
import log from '#main/global/providers/logger';
import { mustParsePortNumberText, parseLaunchctlPidText } from '#main/global/service/boundaryText';
import {
  daemonLabel,
  errorMsg,
  resolveDataDir,
  resolvePortFileName,
} from '#main/global/service/launchd';

const execFileAsync = promisify(execFile);

/** Daemon launchd status. */
export interface DaemonStatus {
  loaded: boolean;
  running: boolean;
  pid: number | null;
}

export async function isServiceLoaded(label: string): Promise<boolean> {
  try {
    await execFileAsync('launchctl', ['list', label]);
    return true;
  } catch {
    return false;
  }
}

const sleep = (ms: number): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));

/** Wait for a daemon port to become free (connection refused = free). */
export async function waitForPortFree(deadline: number): Promise<void> {
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
      await execFileAsync('nc', ['-z', '127.0.0.1', String(port)], {
        timeout: 1000,
      });
      // Connection succeeded — port still in use
      await sleep(500);
    } catch {
      // Connection refused — port is free
      return;
    }
  }
  log.warn(`[launchd] port ${port} still in use after timeout`);
}

/** Poll until a launchd service is fully unloaded (or timeout). */
export async function waitForServiceUnloaded(label: string, timeoutMs = 15000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  // Phase 1: wait for launchd to report unloaded
  while ((await isServiceLoaded(label)) && Date.now() < deadline) {
    await sleep(200);
  }
  if (await isServiceLoaded(label)) {
    log.warn(`[launchd] ${label} still loaded after timeout — proceeding`);
  }
  // Phase 2: wait for port to be free (only for daemon label)
  if (label.includes('daemon') && Date.now() < deadline) {
    await waitForPortFree(deadline);
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
