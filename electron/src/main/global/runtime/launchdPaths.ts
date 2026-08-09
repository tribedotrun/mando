import { app } from 'electron';
import path from 'path';
import fs from 'fs';
import log from '#main/global/providers/logger';
import {
  cargoTargetDir,
  cliInstallPath,
  cliSourcePath,
  daemonInstallPath,
} from '#main/global/service/launchd';

/** Source daemon binary: app bundle or cargo build output. */
function daemonSourcePath(): string {
  if (app.isPackaged) return path.join(process.resourcesPath!, 'mando-gw');
  return path.join(cargoTargetDir(), 'mando-gw');
}

/** Stage a binary from source to install path (atomic: write tmp then rename). */
export function stageBinary(src: string, dest: string, label: string): boolean {
  if (!fs.existsSync(src)) {
    log.warn(`${label} binary not found at ${src}`);
    return false;
  }

  fs.mkdirSync(path.dirname(dest), { recursive: true });
  const tmp = `${dest}.tmp`;
  fs.copyFileSync(src, tmp);
  fs.chmodSync(tmp, 0o755);
  fs.renameSync(tmp, dest);
  return true;
}

/** Stage the daemon binary from app bundle to Application Support. */
export function stageDaemonBinary(): boolean {
  return stageBinary(daemonSourcePath(), daemonInstallPath(), 'daemon');
}

export function copyCliBinary(): void {
  const src = cliSourcePath();
  const dest = cliInstallPath();
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  if (fs.existsSync(src)) {
    fs.copyFileSync(src, dest);
    fs.chmodSync(dest, 0o755);
  }
}

export function stagedDaemonSourcePath(stagedAppPath?: string): string {
  return stagedAppPath
    ? path.join(stagedAppPath, 'Contents', 'Resources', 'mando-gw')
    : daemonSourcePath();
}
