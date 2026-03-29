/**
 * launchd integration — install/manage daemon plist and CLI binary.
 * App login item is managed by Electron's native `app.setLoginItemSettings` API.
 */
import { app } from 'electron';
import path from 'path';
import fs from 'fs';
import { execSync, spawn } from 'child_process';
import log from '#main/logger';

const LEGACY_APP_LABEL = 'run.tribe.mando';
const DAEMON_LABEL = 'run.tribe.mando-daemon';
const TG_LABEL = 'run.tribe.mando-tg';

function homeDir(): string {
  return app.getPath('home');
}

function launchAgentsDir(): string {
  return path.join(homeDir(), 'Library', 'LaunchAgents');
}

function legacyAppPlistPath(): string {
  return path.join(launchAgentsDir(), `${LEGACY_APP_LABEL}.plist`);
}

function daemonPlistPath(): string {
  return path.join(launchAgentsDir(), `${DAEMON_LABEL}.plist`);
}

function tgPlistPath(): string {
  return path.join(launchAgentsDir(), `${TG_LABEL}.plist`);
}

function cliInstallPath(): string {
  return path.join(homeDir(), '.local', 'bin', 'mando');
}

function cliSourcePath(): string {
  if (app.isPackaged) {
    return path.join(process.resourcesPath!, 'mando');
  }
  return path.resolve(__dirname, '../../../target/release/mando');
}

/** Staged daemon binary path in Application Support. */
function daemonInstallPath(): string {
  return path.join(homeDir(), 'Library', 'Application Support', 'Mando', 'bin', 'mando-gw');
}

/** Source daemon binary: app bundle or cargo build output. */
function daemonSourcePath(): string {
  if (app.isPackaged) {
    return path.join(process.resourcesPath!, 'mando-gw');
  }
  return path.resolve(__dirname, '../../../target/release/mando-gw');
}

/** Staged TG bot binary path in Application Support. */
function tgInstallPath(): string {
  return path.join(homeDir(), 'Library', 'Application Support', 'Mando', 'bin', 'mando-tg');
}

/** Source TG bot binary: app bundle or cargo build output. */
function tgSourcePath(): string {
  if (app.isPackaged) {
    return path.join(process.resourcesPath!, 'mando-tg');
  }
  return path.resolve(__dirname, '../../../target/release/mando-tg');
}

function daemonLogDir(): string {
  return path.join(homeDir(), 'Library', 'Logs', 'Mando');
}

function currentPath(): string {
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

/** Remove legacy app LaunchAgent plist. Login item is now managed by Electron's native API. */
export function removeLegacyAppPlist(): void {
  const plist = legacyAppPlistPath();
  if (!fs.existsSync(plist)) return;
  try {
    execSync(`launchctl unload -w "${plist}" 2>/dev/null`);
  } catch {
    /* ok if not loaded */
  }
  try {
    fs.unlinkSync(plist);
    log.info('removed legacy app login plist — now using native Login Items');
  } catch (err) {
    log.warn('[launchd] failed to remove legacy app plist:', err);
  }
}

function generateDaemonPlist(dataDir: string): string {
  const home = homeDir();
  const binary = daemonInstallPath();
  const logDir = daemonLogDir();
  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>${DAEMON_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>${binary}</string>
    </array>
    <key>WorkingDirectory</key>
    <string>${dataDir}</string>
    <key>KeepAlive</key>
    <true/>
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
    <string>${TG_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>${binary}</string>
    </array>
    <key>WorkingDirectory</key>
    <string>${dataDir}</string>
    <key>KeepAlive</key>
    <true/>
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

function launchctlLoad(plistPath: string): void {
  try {
    execSync(`launchctl unload -w "${plistPath}" 2>/dev/null`);
  } catch {
    // Expected if plist is not currently loaded
  }
  execSync(`launchctl load -w "${plistPath}"`);
}

function launchctlUnload(plistPath: string): void {
  try {
    execSync(`launchctl unload -w "${plistPath}"`);
  } catch (e: unknown) {
    log.warn(`launchctl unload failed for ${plistPath}: ${e instanceof Error ? e.message : e}`);
  }
}

/** Bootstrap daemon via launchctl (modern API with fallback). */
function launchctlBootstrap(plistPath: string): void {
  const uid = process.getuid?.() ?? 501;
  const domain = `gui/${uid}`;
  try {
    execSync(`launchctl bootout ${domain}/${DAEMON_LABEL} 2>/dev/null`);
  } catch {
    /* ok if not loaded */
  }
  try {
    execSync(`launchctl bootstrap ${domain} "${plistPath}"`);
  } catch {
    // Fallback to legacy load for older macOS
    launchctlLoad(plistPath);
  }
}

/** Bootout a launchd service by label (modern API with fallback). */
function launchctlBootoutLabel(label: string, plist: string): void {
  const uid = process.getuid?.() ?? 501;
  const domain = `gui/${uid}`;
  try {
    execSync(`launchctl bootout ${domain}/${label}`);
  } catch {
    if (fs.existsSync(plist)) {
      launchctlUnload(plist);
    }
  }
}

/** Bootout daemon via launchctl (modern API with fallback). */
function launchctlBootout(): void {
  launchctlBootoutLabel(DAEMON_LABEL, daemonPlistPath());
}

/** Bootout TG bot via launchctl (modern API with fallback). */
function launchctlBootoutTg(): void {
  launchctlBootoutLabel(TG_LABEL, tgPlistPath());
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

/** Install and load the daemon LaunchAgent plist. */
export function installDaemonPlist(dataDir: string): void {
  ensureLaunchdDirs(dataDir);
  const plist = generateDaemonPlist(dataDir);
  fs.writeFileSync(daemonPlistPath(), plist, 'utf-8');
  launchctlBootstrap(daemonPlistPath());
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
  const plist = generateTgPlist(dataDir);
  fs.writeFileSync(tgPlistPath(), plist, 'utf-8');

  const uid = process.getuid?.() ?? 501;
  const domain = `gui/${uid}`;
  try {
    execSync(`launchctl bootout ${domain}/${TG_LABEL} 2>/dev/null`);
  } catch {
    /* ok if not loaded */
  }
  try {
    execSync(`launchctl bootstrap ${domain} "${tgPlistPath()}"`);
  } catch {
    launchctlLoad(tgPlistPath());
  }
}

/** Update daemon binary: bootout, replace binary, bootstrap. */
export function updateDaemonBinary(dataDir: string): boolean {
  const dest = daemonInstallPath();
  const prev = `${dest}.prev`;

  // Bootout current daemon and TG bot (graceful SIGTERM).
  launchctlBootoutTg();
  launchctlBootout();

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

  launchctlBootoutTg();
  launchctlBootout();
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
    const out = execSync(`launchctl list ${DAEMON_LABEL} 2>/dev/null`, { encoding: 'utf-8' });
    const pidMatch = out.match(/"PID"\s*=\s*(\d+)/);
    return {
      loaded: true,
      running: pidMatch !== null && pidMatch[1] !== '0',
      pid: pidMatch ? parseInt(pidMatch[1], 10) : null,
    };
  } catch {
    return { loaded: false, running: false, pid: null };
  }
}

/** Spawn daemon directly (dev mode, no launchd). */
export function spawnDaemonDev(dataDir: string): ReturnType<typeof spawn> | null {
  const binary = daemonSourcePath();
  if (!fs.existsSync(binary)) {
    log.warn(`daemon binary not found at ${binary}`);
    return null;
  }

  fs.mkdirSync(path.join(dataDir, 'logs'), { recursive: true });

  const child = spawn(binary, ['--foreground', '--dev'], {
    env: {
      ...process.env,
      MANDO_DATA_DIR: dataDir,
      HOME: homeDir(),
    },
    stdio: ['ignore', 'pipe', 'pipe'],
    detached: false,
  });

  child.stdout?.on('data', (data: Buffer) => {
    process.stdout.write(`[daemon] ${data}`);
  });
  child.stderr?.on('data', (data: Buffer) => {
    process.stderr.write(`[daemon] ${data}`);
  });

  return child;
}

// ---------------------------------------------------------------------------
// CLI binary + daemon plist installation
// ---------------------------------------------------------------------------

export function installCliAndPlists(dataDir: string): void {
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

  // 2b. Remove legacy plists (captain cron + app login — both replaced by in-process mechanisms).
  const legacyCaptainPlist = path.join(launchAgentsDir(), 'run.tribe.mando-captain.plist');
  if (fs.existsSync(legacyCaptainPlist)) {
    try {
      execSync(`launchctl unload -w "${legacyCaptainPlist}" 2>/dev/null`);
    } catch {
      /* ok if not loaded */
    }
    fs.unlinkSync(legacyCaptainPlist);
    log.info('removed legacy captain cron plist');
  }
  removeLegacyAppPlist();

  // 3. Stage daemon binary + install daemon plist
  stageDaemonBinary();
  installDaemonPlist(dataDir);

  // 5. Stage TG bot binary + install TG plist
  stageTgBinary();
  installTgPlist(dataDir);
}
