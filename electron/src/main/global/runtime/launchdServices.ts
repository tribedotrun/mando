import path from 'path';
import fs from 'fs';
import { execFile } from 'child_process';
import { promisify } from 'util';
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

const execFileAsync = promisify(execFile);

/** Load a launchd service: bootout first if already loaded, then bootstrap. */
async function launchctlLoad(plistPath: string, label: string): Promise<void> {
  if (await isServiceLoaded(label)) {
    await launchctlBootout(label);
    await waitForServiceUnloaded(label);
  }
  const uid = process.getuid?.() ?? 501;
  await execFileAsync('launchctl', ['bootstrap', `gui/${uid}`, plistPath]);
}

/** Bootout a loaded launchd service. Caller checks isServiceLoaded() first. */
export async function launchctlBootout(label: string): Promise<void> {
  const uid = process.getuid?.() ?? 501;
  try {
    await execFileAsync('launchctl', ['bootout', `gui/${uid}/${label}`]);
  } catch (e: unknown) {
    log.warn(`[launchd] bootout ${label} failed (likely unloaded concurrently):`, errorMsg(e));
  }
}

export async function kickstartDaemon(): Promise<boolean> {
  const label = daemonLabel();
  if (!(await isServiceLoaded(label))) return false;
  const uid = process.getuid?.() ?? 501;
  try {
    await execFileAsync('launchctl', ['kickstart', `gui/${uid}/${label}`]);
    log.info('[launchd] daemon kickstarted');
    return true;
  } catch (e: unknown) {
    // promisify(execFile) errors expose the exit code on `.code` (number for
    // non-zero exits, string for spawn-level errors like 'ENOENT'), not on
    // `.status` like execFileSync did. https://github.com/nodejs/node/issues/7241
    const code = (e as { code?: string | number }).code;
    const stderr = parseNonEmptyText(stderrString(e), 'command:launchctl-kickstart stderr');
    log.warn(`[launchd] kickstart daemon failed (code=${code}): ${stderr ?? errorMsg(e)}`);
    return false;
  }
}

function ensureLaunchdDirs(dataDir: string): void {
  fs.mkdirSync(daemonLogDir(), { recursive: true });
  fs.mkdirSync(launchAgentsDir(), { recursive: true });
  fs.mkdirSync(path.join(dataDir, 'logs'), { recursive: true });
}

async function migrateOldLaunchdLabels(): Promise<void> {
  const oldLabels = ['run.tribe.mando.daemon', 'run.tribe.mando.telegram'];
  for (const label of oldLabels) {
    if (await isServiceLoaded(label)) {
      await launchctlBootout(label);
      await waitForServiceUnloaded(label);
      log.info(`[launchd] migrated legacy service: ${label}`);
    }
    const plist = path.join(launchAgentsDir(), `${label}.plist`);
    try {
      await fs.promises.unlink(plist);
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

export async function cleanupTelegramArtifacts(): Promise<void> {
  const label = isPreview()
    ? 'build.mando.preview.telegram'
    : isDev()
      ? 'build.mando.telegram.dev'
      : 'build.mando.telegram';
  if (await isServiceLoaded(label)) {
    await launchctlBootout(label);
    await waitForServiceUnloaded(label);
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
      await fs.promises.unlink(file);
    } catch (e: unknown) {
      const code = (e as NodeJS.ErrnoException)?.code;
      if (code !== 'ENOENT') {
        log.warn(`[launchd] failed to remove deprecated Telegram artifact ${file}: ${errorMsg(e)}`);
      }
    }
  }
}

/** Install and load the daemon LaunchAgent plist. */
export async function installDaemonPlist(dataDir: string): Promise<void> {
  await migrateOldLaunchdLabels();
  await cleanupTelegramArtifacts();
  ensureLaunchdDirs(dataDir);
  const plistFile = daemonPlistPath();
  fs.writeFileSync(plistFile, generateDaemonPlist(dataDir), 'utf-8');
  await launchctlLoad(plistFile, daemonLabel());
}
