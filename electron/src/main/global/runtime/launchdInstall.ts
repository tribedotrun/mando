import path from 'path';
import fs from 'fs';
import log from '#main/global/providers/logger';
import { isServiceLoaded, waitForServiceUnloaded } from '#main/global/runtime/portCheck';
import { daemonLabel, daemonInstallPath } from '#main/global/service/launchd';
import {
  cleanupTelegramArtifacts,
  installDaemonPlist,
  launchctlBootout,
} from '#main/global/runtime/launchdServices';
import {
  copyCliBinary,
  stageBinary,
  stageDaemonBinary,
  stagedDaemonSourcePath,
} from '#main/global/runtime/launchdPaths';

/** Update daemon binary: bootout, replace binary, bootstrap.
 *  When `stagedAppPath` is provided, binaries are copied from the staged app
 *  bundle instead of the currently running app — used by the update flow to
 *  install the NEW binary before swapping the .app bundle. */
export function updateDaemonBinary(dataDir: string, stagedAppPath?: string): boolean {
  const dest = daemonInstallPath();
  const prev = `${dest}.prev`;

  const label = daemonLabel();
  if (isServiceLoaded(label)) {
    launchctlBootout(label);
    waitForServiceUnloaded(label);
  }
  cleanupTelegramArtifacts();

  if (fs.existsSync(dest)) {
    try {
      fs.renameSync(dest, prev);
    } catch (err) {
      log.warn('[launchd] failed to backup current binary:', err);
    }
  }

  if (!stageBinary(stagedDaemonSourcePath(stagedAppPath), dest, 'daemon')) {
    if (fs.existsSync(prev)) {
      try {
        fs.renameSync(prev, dest);
      } catch (err) {
        log.warn('[launchd] rollback rename failed:', err);
      }
    }
    return false;
  }

  installDaemonPlist(dataDir);
  return true;
}

/** Rollback to previous daemon binary if available. */
export function rollbackDaemonBinary(dataDir: string): boolean {
  const dest = daemonInstallPath();
  const prev = `${dest}.prev`;
  if (!fs.existsSync(prev)) return false;

  const label = daemonLabel();
  if (isServiceLoaded(label)) {
    launchctlBootout(label);
    waitForServiceUnloaded(label);
  }
  cleanupTelegramArtifacts();
  try {
    fs.renameSync(prev, dest);
  } catch (err) {
    log.warn('[launchd] rollback rename failed:', err);
    return false;
  }
  installDaemonPlist(dataDir);
  return true;
}

export function installCliAndPlists(dataDir: string, opts?: { skipDaemonPlist?: boolean }): void {
  copyCliBinary();
  fs.mkdirSync(path.join(dataDir, 'logs'), { recursive: true });

  if (!opts?.skipDaemonPlist) {
    stageDaemonBinary();
    installDaemonPlist(dataDir);
  }
}
