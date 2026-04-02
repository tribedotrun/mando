/**
 * launchd integration — install/manage daemon plist and CLI binary.
 * App login item is managed by Electron's native `app.setLoginItemSettings` API.
 */
import { app } from 'electron';
import path from 'path';
import fs from 'fs';
import { execSync, spawn } from 'child_process';
import log from '#main/logger';

const DAEMON_LABEL = 'run.tribe.mando.daemon';
const TG_LABEL = 'run.tribe.mando.telegram';

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
  return path.join(homeDir(), 'Library', 'Application Support', 'Mando', 'bin', 'mando-daemon');
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
  return path.join(homeDir(), 'Library', 'Application Support', 'Mando', 'bin', 'mando-telegram');
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

/** Load a launchd service: bootout first if already loaded, then bootstrap. */
function launchctlLoad(plistPath: string, label: string): void {
  if (isServiceLoaded(label)) launchctlBootout(label);
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

/** Remove old-named binaries from before the space-to-dash rename. */
function cleanupLegacyBinaries(): void {
  const binDir = path.join(homeDir(), 'Library', 'Application Support', 'Mando', 'bin');
  for (const old of [
    'Mando Daemon',
    'Mando Telegram',
    'Mando Daemon.prev',
    'Mando Telegram.prev',
  ]) {
    const p = path.join(binDir, old);
    try {
      if (fs.existsSync(p)) fs.unlinkSync(p);
    } catch {
      /* best-effort cleanup */
    }
  }
}

/** Stage the daemon binary from app bundle to Application Support. */
export function stageDaemonBinary(): boolean {
  cleanupLegacyBinaries();
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
  const plistFile = daemonPlistPath();
  fs.writeFileSync(plistFile, generateDaemonPlist(dataDir), 'utf-8');
  launchctlLoad(plistFile, DAEMON_LABEL);
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
  launchctlLoad(plistFile, TG_LABEL);
}

/** Update daemon binary: bootout, replace binary, bootstrap. */
export function updateDaemonBinary(dataDir: string): boolean {
  const dest = daemonInstallPath();
  const prev = `${dest}.prev`;

  // Bootout running services before replacing binaries.
  if (isServiceLoaded(TG_LABEL)) launchctlBootout(TG_LABEL);
  if (isServiceLoaded(DAEMON_LABEL)) launchctlBootout(DAEMON_LABEL);

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

  if (isServiceLoaded(TG_LABEL)) launchctlBootout(TG_LABEL);
  if (isServiceLoaded(DAEMON_LABEL)) launchctlBootout(DAEMON_LABEL);
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

  // 3. Stage daemon binary + install daemon plist
  stageDaemonBinary();
  installDaemonPlist(dataDir);

  // 5. Stage TG bot binary + install TG plist
  stageTgBinary();
  installTgPlist(dataDir);
}
