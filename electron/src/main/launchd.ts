/**
 * launchd integration — install/manage daemon plist and CLI binary.
 * App login item is managed by Electron's native `app.setLoginItemSettings` API.
 */
import { app } from 'electron';
import path from 'path';
import fs from 'fs';
import { execSync } from 'child_process';
import log from '#main/logger';
import { isServiceLoaded, waitForServiceUnloaded } from '#main/port-check';
export type { DaemonStatus } from '#main/port-check';
export { getDaemonStatus } from '#main/port-check';

// ---------------------------------------------------------------------------
// Mode-aware identifiers — dev uses `.dev` suffixed labels and separate
// binary paths so dev and prod can coexist without collisions.
// ---------------------------------------------------------------------------

function isDev(): boolean {
  return process.env.MANDO_APP_MODE === 'dev';
}

function isPreview(): boolean {
  return process.env.MANDO_APP_MODE === 'preview' || process.execPath.includes('Mando (Preview)');
}

function daemonLabel(): string {
  if (isPreview()) return 'build.mando.preview.daemon';
  return isDev() ? 'build.mando.daemon.dev' : 'build.mando.daemon';
}

/** Extract message string from an unknown error. */
function errorMsg(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** Normalize an execSync error's stderr (Buffer or string) to a plain string. */
function stderrString(e: unknown): string {
  const raw = (e as { stderr?: unknown }).stderr;
  if (raw == null) return '';
  if (typeof raw === 'string') return raw;
  if (Buffer.isBuffer(raw)) return raw.toString('utf-8');
  return String(raw);
}

function homeDir(): string {
  return app.getPath('home');
}

function launchAgentsDir(): string {
  return path.join(homeDir(), 'Library', 'LaunchAgents');
}

function daemonPlistPath(): string {
  return path.join(launchAgentsDir(), `${daemonLabel()}.plist`);
}

function cliInstallPath(): string {
  const name = isPreview() ? 'mando-preview' : isDev() ? 'mando-dev' : 'mando';
  return path.join(homeDir(), '.local', 'bin', name);
}

/** Resolve cargo target dir — respects env overrides, then walks up to workspace root. */
function cargoTargetDir(): string {
  const override = process.env.MANDO_RUST_TARGET_DIR || process.env.CARGO_TARGET_DIR;
  if (override) return override;
  // Walk up from the electron dir to find the workspace Cargo.toml → target/debug.
  // In worktrees the target dir lives at the primary repo root, not the worktree.
  let dir = path.resolve(__dirname, '../../..');
  for (let i = 0; i < 5; i++) {
    if (fs.existsSync(path.join(dir, 'target', 'debug', 'mando-gw'))) {
      return path.join(dir, 'target', 'debug');
    }
    dir = path.dirname(dir);
  }
  // Fallback: assume target is relative to the repo root (standard layout).
  return path.resolve(__dirname, '../../../target/debug');
}

function cliSourcePath(): string {
  if (app.isPackaged) return path.join(process.resourcesPath!, 'mando');
  return path.join(cargoTargetDir(), 'mando');
}

/** Staged daemon binary path in Application Support. */
function daemonInstallPath(): string {
  const name = isPreview() ? 'mando-daemon-preview' : isDev() ? 'mando-daemon-dev' : 'mando-daemon';
  return path.join(homeDir(), 'Library', 'Application Support', 'Mando', 'bin', name);
}

/** Source daemon binary: app bundle or cargo build output. */
function daemonSourcePath(): string {
  if (app.isPackaged) return path.join(process.resourcesPath!, 'mando-gw');
  return path.join(cargoTargetDir(), 'mando-gw');
}

function daemonLogDir(): string {
  const dir = isPreview() ? 'Mando-Preview' : isDev() ? 'Mando-Dev' : 'Mando';
  return path.join(homeDir(), 'Library', 'Logs', dir);
}

export function currentPath(): string {
  const base = [
    path.join(homeDir(), '.local', 'bin'),
    '/opt/homebrew/bin',
    '/usr/local/bin',
    '/usr/bin',
    '/bin',
  ];
  // Include nvm node if present
  const nvmNode = process.env.NVM_BIN;
  if (nvmNode) base.splice(1, 0, nvmNode);
  return base.join(':');
}

function generateDaemonPlist(dataDir: string): string {
  const home = homeDir();
  const binary = daemonInstallPath();
  const logDir = daemonLogDir();
  let extraArgs = '';
  if (isDev())
    extraArgs =
      '\n        <string>--dev</string>\n        <string>--port</string>\n        <string>18600</string>';
  if (isPreview()) extraArgs = '\n        <string>--port</string>\n        <string>18650</string>';
  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>${daemonLabel()}</string>
    <key>ProgramArguments</key>
    <array>
        <string>${binary}</string>${extraArgs}
    </array>
    <key>WorkingDirectory</key>
    <string>${dataDir}</string>
    <key>KeepAlive</key>
    <true/>
    <key>ThrottleInterval</key>
    <integer>3</integer>
    <key>StandardOutPath</key>
    <string>${path.join(logDir, 'daemon.log')}</string>
    <key>StandardErrorPath</key>
    <string>${path.join(logDir, 'daemon.log')}</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>HOME</key>
        <string>${home}</string>
        <key>MANDO_DATA_DIR</key>
        <string>${dataDir}</string>
        <key>PATH</key>
        <string>${currentPath()}</string>
    </dict>
</dict>
</plist>`;
}

/** Load a launchd service: bootout first if already loaded, then bootstrap. */
function launchctlLoad(plistPath: string, label: string): void {
  if (isServiceLoaded(label)) {
    launchctlBootout(label);
    // bootout is async at the OS level — it returns before the process fully
    // exits and releases resources (ports, PID files). Bootstrapping
    // immediately would race with the dying process.
    waitForServiceUnloaded(label);
  }
  const uid = process.getuid?.() ?? 501;
  execSync(`launchctl bootstrap gui/${uid} "${plistPath}"`);
}

/** Bootout a loaded launchd service. Caller checks isServiceLoaded() first. */
function launchctlBootout(label: string): void {
  const uid = process.getuid?.() ?? 501;
  try {
    execSync(`launchctl bootout gui/${uid}/${label}`);
  } catch (e: unknown) {
    // TOCTOU: service unloaded between isServiceLoaded() and bootout.
    // Log and continue — the goal (service not loaded) is achieved.
    log.warn(`[launchd] bootout ${label} failed (likely unloaded concurrently):`, errorMsg(e));
  }
}

// Kickstart the daemon service, telling launchd to start it immediately,
// bypassing any crash-loop throttle. No-op if the service is not loaded.
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
    const stderr = stderrString(e);
    log.warn(
      `[launchd] kickstart daemon failed (status=${status}): ${stderr.trim() || errorMsg(e)}`,
    );
    return false;
  }
}

// ---------------------------------------------------------------------------
// Daemon binary staging + plist management
// ---------------------------------------------------------------------------

/** Stage a binary from source to install path (atomic: write tmp then rename). */
function stageBinary(src: string, dest: string, label: string): boolean {
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

/** Ensure shared directories exist for launchd services. */
function ensureLaunchdDirs(dataDir: string): void {
  fs.mkdirSync(daemonLogDir(), { recursive: true });
  fs.mkdirSync(launchAgentsDir(), { recursive: true });
  fs.mkdirSync(path.join(dataDir, 'logs'), { recursive: true });
}

// Bootout and remove legacy launchd services from before the label rename.
// Safe to call repeatedly, no-ops when the old services are not loaded.
function migrateOldLaunchdLabels(): void {
  const oldLabels = ['run.tribe.mando.daemon', 'run.tribe.mando.telegram'];
  for (const label of oldLabels) {
    if (isServiceLoaded(label)) {
      launchctlBootout(label);
      waitForServiceUnloaded(label);
      log.info(`[launchd] migrated legacy service: ${label}`);
    }
    // Remove old plist file. ENOENT is normal (migration already ran on a
    // prior launch); anything else hints at permission or filesystem issues
    // we want visible without spamming debug logs for missing files.
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

function cleanupTelegramArtifacts(): void {
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

/** Update daemon binary: bootout, replace binary, bootstrap.
 *  When `stagedAppPath` is provided, binaries are copied from the staged app
 *  bundle instead of the currently running app — used by the update flow to
 *  install the NEW binary before swapping the .app bundle. */
export function updateDaemonBinary(dataDir: string, stagedAppPath?: string): boolean {
  const dest = daemonInstallPath();
  const prev = `${dest}.prev`;

  // Bootout running services before replacing binaries.
  // Wait for each to fully unload — bootout is async at the OS level.
  const dl = daemonLabel();
  if (isServiceLoaded(dl)) {
    launchctlBootout(dl);
    waitForServiceUnloaded(dl);
  }
  cleanupTelegramArtifacts();

  // Rename current binary to .prev for rollback.
  if (fs.existsSync(dest)) {
    try {
      fs.renameSync(dest, prev);
    } catch (err) {
      log.warn('[launchd] failed to backup current binary:', err);
    }
  }

  // Copy new binaries — from the staged app bundle if provided, else current.
  const gwSrc = stagedAppPath
    ? path.join(stagedAppPath, 'Contents', 'Resources', 'mando-gw')
    : daemonSourcePath();
  if (!stageBinary(gwSrc, dest, 'daemon')) {
    // Rollback on failure.
    if (fs.existsSync(prev)) {
      try {
        fs.renameSync(prev, dest);
      } catch (err) {
        log.warn('[launchd] rollback rename failed:', err);
      }
    }
    return false;
  }

  // Bootstrap updated daemon.
  installDaemonPlist(dataDir);
  return true;
}

/** Rollback to previous daemon binary if available. */
export function rollbackDaemonBinary(dataDir: string): boolean {
  const dest = daemonInstallPath();
  const prev = `${dest}.prev`;
  if (!fs.existsSync(prev)) return false;

  const dl = daemonLabel();
  if (isServiceLoaded(dl)) {
    launchctlBootout(dl);
    waitForServiceUnloaded(dl);
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

// ---------------------------------------------------------------------------
// CLI binary + daemon plist installation
// ---------------------------------------------------------------------------

export function installCliAndPlists(dataDir: string, opts?: { skipDaemonPlist?: boolean }): void {
  // 1. Copy CLI binary
  const src = cliSourcePath();
  const dest = cliInstallPath();
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  if (fs.existsSync(src)) {
    fs.copyFileSync(src, dest);
    fs.chmodSync(dest, 0o755);
  }

  // 2. Ensure dirs exist
  fs.mkdirSync(path.join(dataDir, 'logs'), { recursive: true });
  fs.mkdirSync(launchAgentsDir(), { recursive: true });

  // 3. Stage daemon binary + install daemon plist
  //    When skipDaemonPlist is set, ensureDaemon already staged the binary and
  //    bootstrapped the service — re-staging would overwrite the running binary.
  if (!opts?.skipDaemonPlist) {
    stageDaemonBinary();
    installDaemonPlist(dataDir);
  }
}
