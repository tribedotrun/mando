import path from 'path';
import fs from 'fs';
import { execSync } from 'child_process';
import log from '#main/global/providers/logger';
import { isServiceLoaded, waitForServiceUnloaded } from '#main/global/runtime/portCheck';
import { parseNonEmptyText } from '#main/global/service/boundaryText';
import {
  isDev,
  isPreview,
  daemonLabel,
  errorMsg,
  stderrString,
  homeDir,
  launchAgentsDir,
  daemonPlistPath,
  daemonLogDir,
  generateDaemonPlist,
} from '#main/global/service/launchd';

/** Load a launchd service: bootout first if already loaded, then bootstrap. */
export function launchctlLoad(plistPath: string, label: string): void {
  if (isServiceLoaded(label)) {
    launchctlBootout(label);
    waitForServiceUnloaded(label);
  }
  const uid = process.getuid?.() ?? 501;
  execSync(`launchctl bootstrap gui/${uid} "${plistPath}"`);
}

/** Bootout a loaded launchd service. Caller checks isServiceLoaded() first. */
export function launchctlBootout(label: string): void {
  const uid = process.getuid?.() ?? 501;
  try {
    execSync(`launchctl bootout gui/${uid}/${label}`);
  } catch (e: unknown) {
    log.warn(`[launchd] bootout ${label} failed (likely unloaded concurrently):`, errorMsg(e));
  }
}

export function kickstartDaemon(): boolean {
  const label = daemonLabel();
  if (!isServiceLoaded(label)) return false;
  const uid = process.getuid?.() ?? 501;
  try {
    execSync(`launchctl kickstart gui/${uid}/${label}`, {
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    log.info('[launchd] daemon kickstarted');
    return true;
  } catch (e: unknown) {
    const status = (e as { status?: number }).status;
    const stderr = parseNonEmptyText(stderrString(e), 'command:launchctl-kickstart stderr');
    log.warn(`[launchd] kickstart daemon failed (status=${status}): ${stderr ?? errorMsg(e)}`);
    return false;
  }
}

function ensureLaunchdDirs(dataDir: string): void {
  fs.mkdirSync(daemonLogDir(), { recursive: true });
  fs.mkdirSync(launchAgentsDir(), { recursive: true });
  fs.mkdirSync(path.join(dataDir, 'logs'), { recursive: true });
}

function migrateOldLaunchdLabels(): void {
  const oldLabels = ['run.tribe.mando.daemon', 'run.tribe.mando.telegram'];
  for (const label of oldLabels) {
    if (isServiceLoaded(label)) {
      launchctlBootout(label);
      waitForServiceUnloaded(label);
      log.info(`[launchd] migrated legacy service: ${label}`);
    }
    const plist = path.join(launchAgentsDir(), `${label}.plist`);
    try {
      fs.unlinkSync(plist);
    } catch (e: unknown) {
      const code = (e as NodeJS.ErrnoException)?.code;
      if (code === 'ENOENT') {
        log.debug(`[launchd] legacy plist ${label} already absent`);
      } else {
        log.warn(`[launchd] failed to remove legacy plist ${plist}: ${errorMsg(e)}`);
      }
    }
  }
}

export function cleanupTelegramArtifacts(): void {
  const label = isPreview()
    ? 'build.mando.preview.telegram'
    : isDev()
      ? 'build.mando.telegram.dev'
      : 'build.mando.telegram';
  if (isServiceLoaded(label)) {
    launchctlBootout(label);
    waitForServiceUnloaded(label);
    log.info(`[launchd] removed deprecated Telegram service: ${label}`);
  }

  const plistPath = path.join(launchAgentsDir(), `${label}.plist`);
  const tgInstallName = isPreview()
    ? 'mando-telegram-preview'
    : isDev()
      ? 'mando-telegram-dev'
      : 'mando-telegram';
  const tgBinaryPath = path.join(
    homeDir(),
    'Library',
    'Application Support',
    'Mando',
    'bin',
    tgInstallName,
  );

  for (const file of [plistPath, tgBinaryPath]) {
    try {
      fs.unlinkSync(file);
    } catch (e: unknown) {
      const code = (e as NodeJS.ErrnoException)?.code;
      if (code !== 'ENOENT') {
        log.warn(`[launchd] failed to remove deprecated Telegram artifact ${file}: ${errorMsg(e)}`);
      }
    }
  }
}

/** Install and load the daemon LaunchAgent plist. */
export function installDaemonPlist(dataDir: string): void {
  migrateOldLaunchdLabels();
  cleanupTelegramArtifacts();
  ensureLaunchdDirs(dataDir);
  const plistFile = daemonPlistPath();
  fs.writeFileSync(plistFile, generateDaemonPlist(dataDir), 'utf-8');
  launchctlLoad(plistFile, daemonLabel());
}
