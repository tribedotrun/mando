/**
 * launchd integration — install/manage daemon plist and CLI binary.
 * App login item is managed by Electron's native `app.setLoginItemSettings` API.
 */
import { app } from 'electron';
import path from 'path';
import fs from 'fs';
import { execSync } from 'child_process';
import log from '#main/logger';

// ---------------------------------------------------------------------------
// Mode-aware identifiers — dev uses `.dev` suffixed labels and separate
// binary paths so dev and prod can coexist without collisions.
// ---------------------------------------------------------------------------

function isDev(): boolean {
  return process.env.MANDO_APP_MODE === 'dev';
}

function daemonLabel(): string {
  return isDev() ? 'build.mando.daemon.dev' : 'build.mando.daemon';
}

function tgLabel(): string {
  return isDev() ? 'build.mando.telegram.dev' : 'build.mando.telegram';
}

/** Extract message string from an unknown error. */
function errorMsg(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export function homeDir(): string {
  return app.getPath('home');
}

function launchAgentsDir(): string {
  return path.join(homeDir(), 'Library', 'LaunchAgents');
}

function daemonPlistPath(): string {
  return path.join(launchAgentsDir(), `${daemonLabel()}.plist`);
}

function tgPlistPath(): string {
  return path.join(launchAgentsDir(), `${tgLabel()}.plist`);
}

function cliInstallPath(): string {
  return path.join(homeDir(), '.local', 'bin', isDev() ? 'mando-dev' : 'mando');
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
  const name = isDev() ? 'mando-daemon-dev' : 'mando-daemon';
  return path.join(homeDir(), 'Library', 'Application Support', 'Mando', 'bin', name);
}

/** Source daemon binary: app bundle or cargo build output. */
function daemonSourcePath(): string {
  if (app.isPackaged) return path.join(process.resourcesPath!, 'mando-gw');
  return path.join(cargoTargetDir(), 'mando-gw');
}

/** Staged TG bot binary path in Application Support. */
function tgInstallPath(): string {
  const name = isDev() ? 'mando-telegram-dev' : 'mando-telegram';
  return path.join(homeDir(), 'Library', 'Application Support', 'Mando', 'bin', name);
}

/** Source TG bot binary: app bundle or cargo build output. */
function tgSourcePath(): string {
  if (app.isPackaged) return path.join(process.resourcesPath!, 'mando-tg');
  return path.join(cargoTargetDir(), 'mando-tg');
}

function daemonLogDir(): string {
  return path.join(homeDir(), 'Library', 'Logs', isDev() ? 'Mando-Dev' : 'Mando');
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
  const devArg = isDev() ? '\n        <string>--dev</string>' : '';
  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>${daemonLabel()}</string>
    <key>ProgramArguments</key>
    <array>
        <string>${binary}</string>${devArg}
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

function generateTgPlist(dataDir: string): string {
  const home = homeDir();
  const binary = tgInstallPath();
  const logDir = daemonLogDir();
  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>${tgLabel()}</string>
    <key>ProgramArguments</key>
    <array>
        <string>${binary}</string>
    </array>
    <key>WorkingDirectory</key>
    <string>${dataDir}</string>
    <key>KeepAlive</key>
    <true/>
    <key>ThrottleInterval</key>
    <integer>3</integer>
    <key>StandardOutPath</key>
    <string>${path.join(logDir, 'tg-bot.log')}</string>
    <key>StandardErrorPath</key>
    <string>${path.join(logDir, 'tg-bot.log')}</string>
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

/** Check whether a launchd service is currently loaded. */
function isServiceLoaded(label: string): boolean {
  try {
    execSync(`launchctl list ${label}`, { stdio: 'pipe' });
    return true;
  } catch {
    return false;
  }
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

/** Poll until a launchd service is fully unloaded (or timeout). */
function waitForServiceUnloaded(label: string, timeoutMs = 5000): void {
  const deadline = Date.now() + timeoutMs;
  while (isServiceLoaded(label) && Date.now() < deadline) {
    execSync('sleep 0.2', { stdio: 'pipe' });
  }
  if (isServiceLoaded(label)) {
    log.warn(`[launchd] ${label} still loaded after ${timeoutMs}ms — proceeding with bootstrap`);
  }
}

/** Kickstart the daemon service — tells launchd to start it immediately,
 *  bypassing any crash-loop throttle. No-op if the service is not loaded. */
export function kickstartDaemon(): boolean {
  const label = daemonLabel();
  if (!isServiceLoaded(label)) return false;
  const uid = process.getuid?.() ?? 501;
  try {
    execSync(`launchctl kickstart gui/${uid}/${label}`, { stdio: 'pipe' });
    log.info('[launchd] daemon kickstarted');
    return true;
  } catch (e: unknown) {
    log.warn('[launchd] kickstart daemon failed:', errorMsg(e));
    return false;
  }
}

// ---------------------------------------------------------------------------
// Daemon binary staging + plist management
// ---------------------------------------------------------------------------

/** Stage a binary from source to install path. */
function stageBinary(src: string, dest: string, label: string): boolean {
  if (!fs.existsSync(src)) {
    log.warn(`${label} binary not found at ${src}`);
    return false;
  }

  fs.mkdirSync(path.dirname(dest), { recursive: true });
  fs.copyFileSync(src, dest);
  fs.chmodSync(dest, 0o755);
  return true;
}

/** Stage the daemon binary from app bundle to Application Support. */
export function stageDaemonBinary(): boolean {
  return stageBinary(daemonSourcePath(), daemonInstallPath(), 'daemon');
}

/** Stage the TG bot binary from app bundle to Application Support. */
export function stageTgBinary(): boolean {
  return stageBinary(tgSourcePath(), tgInstallPath(), 'tg-bot');
}

/** Ensure shared directories exist for launchd services. */
function ensureLaunchdDirs(dataDir: string): void {
  fs.mkdirSync(daemonLogDir(), { recursive: true });
  fs.mkdirSync(launchAgentsDir(), { recursive: true });
  fs.mkdirSync(path.join(dataDir, 'logs'), { recursive: true });
}

/** Bootout and remove legacy launchd services from before the label rename.
 *  Safe to call repeatedly — no-ops when the old services are not loaded. */
function migrateOldLaunchdLabels(): void {
  const oldLabels = ['run.tribe.mando.daemon', 'run.tribe.mando.telegram'];
  for (const label of oldLabels) {
    if (isServiceLoaded(label)) {
      launchctlBootout(label);
      waitForServiceUnloaded(label);
      log.info(`[launchd] migrated legacy service: ${label}`);
    }
    // Remove old plist file.
    const plist = path.join(launchAgentsDir(), `${label}.plist`);
    try {
      if (fs.existsSync(plist)) fs.unlinkSync(plist);
    } catch {
      /* best-effort */
    }
  }
}

/** Install and load the daemon LaunchAgent plist. */
export function installDaemonPlist(dataDir: string): void {
  migrateOldLaunchdLabels();
  ensureLaunchdDirs(dataDir);
  const plistFile = daemonPlistPath();
  fs.writeFileSync(plistFile, generateDaemonPlist(dataDir), 'utf-8');
  launchctlLoad(plistFile, daemonLabel());
}

/** Install and load the TG bot LaunchAgent plist — skips if Telegram is not configured. */
export function installTgPlist(dataDir: string): void {
  // Only install the TG bot service if Telegram is actually configured.
  // Without a token, mando-tg will crash-loop under KeepAlive.
  const configPath = path.join(dataDir, 'config.json');
  try {
    const raw = fs.readFileSync(configPath, 'utf-8');
    const cfg = JSON.parse(raw) as {
      channels?: { telegram?: { enabled?: boolean } };
      env?: Record<string, string>;
    };
    const enabled = cfg.channels?.telegram?.enabled ?? false;
    const hasToken = !!cfg.env?.TELEGRAM_MANDO_BOT_TOKEN;
    if (!enabled || !hasToken) {
      log.info('[launchd] Skipping TG plist — Telegram not configured');
      return;
    }
  } catch {
    log.info('[launchd] Skipping TG plist — cannot read config');
    return;
  }

  ensureLaunchdDirs(dataDir);
  const plistFile = tgPlistPath();
  fs.writeFileSync(plistFile, generateTgPlist(dataDir), 'utf-8');
  launchctlLoad(plistFile, tgLabel());
}

/** Update daemon binary: bootout, replace binary, bootstrap. */
export function updateDaemonBinary(dataDir: string): boolean {
  const dest = daemonInstallPath();
  const prev = `${dest}.prev`;

  // Bootout running services before replacing binaries.
  // Wait for each to fully unload — bootout is async at the OS level.
  const tl = tgLabel();
  const dl = daemonLabel();
  if (isServiceLoaded(tl)) {
    launchctlBootout(tl);
    waitForServiceUnloaded(tl);
  }
  if (isServiceLoaded(dl)) {
    launchctlBootout(dl);
    waitForServiceUnloaded(dl);
  }

  // Rename current binary to .prev for rollback.
  if (fs.existsSync(dest)) {
    try {
      fs.renameSync(dest, prev);
    } catch (err) {
      log.warn('[launchd] failed to backup current binary:', err);
    }
  }

  // Copy new binaries from app bundle.
  if (!stageDaemonBinary()) {
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

  // Stage TG binary (non-fatal if missing — user may not use Telegram).
  stageTgBinary();

  // Bootstrap updated daemon + TG bot.
  installDaemonPlist(dataDir);
  installTgPlist(dataDir);
  return true;
}

/** Rollback to previous daemon binary if available. */
export function rollbackDaemonBinary(dataDir: string): boolean {
  const dest = daemonInstallPath();
  const prev = `${dest}.prev`;
  if (!fs.existsSync(prev)) return false;

  const tl = tgLabel();
  const dl = daemonLabel();
  if (isServiceLoaded(tl)) {
    launchctlBootout(tl);
    waitForServiceUnloaded(tl);
  }
  if (isServiceLoaded(dl)) {
    launchctlBootout(dl);
    waitForServiceUnloaded(dl);
  }
  try {
    fs.renameSync(prev, dest);
  } catch (err) {
    log.warn('[launchd] rollback rename failed:', err);
    return false;
  }
  installDaemonPlist(dataDir);
  installTgPlist(dataDir);
  return true;
}

/** Get daemon status via launchctl. */
interface DaemonStatus {
  loaded: boolean;
  running: boolean;
  pid: number | null;
}

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
    // Log anything else so real errors aren't silent.
    const msg = errorMsg(e);
    if (!msg.includes('Could not find service')) {
      log.warn('[launchd] daemon status check failed:', msg);
    }
    return { loaded: false, running: false, pid: null };
  }
}

/** Bootout dev-mode launchd services on quit. No-op in prod
 *  (prod daemon persists across Electron restarts via KeepAlive). */
export function bootoutDevServices(): void {
  if (!isDev()) return;
  const dl = daemonLabel();
  const tl = tgLabel();
  if (isServiceLoaded(dl)) launchctlBootout(dl);
  if (isServiceLoaded(tl)) launchctlBootout(tl);
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

  // 4. Stage TG bot binary + install TG plist
  stageTgBinary();
  installTgPlist(dataDir);
}
