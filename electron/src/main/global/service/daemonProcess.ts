import fs from 'fs';
import path from 'path';
import { SIGTERM_POLL_MS, SIGKILL_SETTLE_MS } from '#main/global/config/lifecycle';
import { mustParsePositiveIntegerText } from '#main/global/service/boundaryText';
import { isProcessAlive } from '#main/global/service/lifecycle';

function cleanupStaleDaemonFiles(dataDir: string): void {
  for (const file of ['daemon.pid', 'daemon.port', 'daemon-dev.port', 'daemon-preview.port']) {
    try {
      fs.unlinkSync(path.join(dataDir, file));
    } catch {
      continue;
    }
  }
}

export async function killDaemonByPid(pid: number, dataDir: string) {
  try {
    process.kill(pid, 'SIGTERM');
  } catch (err: unknown) {
    const code = (err as NodeJS.ErrnoException).code;
    if (code === 'ESRCH') {
      cleanupStaleDaemonFiles(dataDir);
      return true;
    }
    return false;
  }

  for (let i = 0; i < 12; i++) {
    try {
      process.kill(pid, 0);
    } catch (err: unknown) {
      if ((err as NodeJS.ErrnoException).code === 'ESRCH') break;
      return false;
    }
    await new Promise((resolve) => setTimeout(resolve, SIGTERM_POLL_MS));
  }

  try {
    process.kill(pid, 0);
    process.kill(pid, 'SIGKILL');
    await new Promise((resolve) => setTimeout(resolve, SIGKILL_SETTLE_MS));
  } catch (err: unknown) {
    if ((err as NodeJS.ErrnoException | null)?.code !== 'ESRCH') return false;
  }

  if (isProcessAlive(pid)) {
    return false;
  }

  cleanupStaleDaemonFiles(dataDir);
  return true;
}

export function readExistingDaemonPid(dataDir: string): number | null {
  const pidFile = path.join(dataDir, 'daemon.pid');
  try {
    return mustParsePositiveIntegerText(fs.readFileSync(pidFile, 'utf-8'), `file:${pidFile}`);
  } catch {
    return null;
  }
}

export function hasDaemonConfig(dataDir: string): boolean {
  return fs.existsSync(path.join(dataDir, 'config.json'));
}
