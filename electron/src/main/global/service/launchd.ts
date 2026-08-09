import { app } from 'electron';
import fs from 'fs';
import os from 'os';
import path from 'path';

export function isDev(): boolean {
  return process.env.MANDO_APP_MODE === 'dev';
}

export function isPreview(): boolean {
  return process.env.MANDO_APP_MODE === 'preview' || process.execPath.includes('Mando (Preview)');
}

export function daemonLabel(): string {
  if (isPreview()) return 'build.mando.preview.daemon';
  return isDev() ? 'build.mando.daemon.dev' : 'build.mando.daemon';
}

export function errorMsg(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export function stderrString(e: unknown): string {
  const raw = (e as { stderr?: unknown }).stderr;
  if (raw == null) return '';
  if (typeof raw === 'string') return raw;
  if (Buffer.isBuffer(raw)) return raw.toString('utf-8');
  return String(raw);
}

export function homeDir(): string {
  return os.homedir();
}

export function launchAgentsDir(): string {
  return path.join(homeDir(), 'Library', 'LaunchAgents');
}

export function daemonPlistPath(): string {
  return path.join(launchAgentsDir(), `${daemonLabel()}.plist`);
}

export function cliInstallPath(): string {
  const name = isPreview() ? 'mando-preview' : isDev() ? 'mando-dev' : 'mando';
  return path.join(homeDir(), '.local', 'bin', name);
}

export function daemonInstallPath(): string {
  const name = isPreview() ? 'mando-daemon-preview' : isDev() ? 'mando-daemon-dev' : 'mando-daemon';
  return path.join(homeDir(), 'Library', 'Application Support', 'Mando', 'bin', name);
}

export function daemonLogDir(): string {
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

export function resolveDataDir(): string {
  if (process.env.MANDO_DATA_DIR) return process.env.MANDO_DATA_DIR;
  if (isPreview()) return path.join(homeDir(), '.mando-preview');
  if (isDev()) return path.join(homeDir(), '.mando-dev');
  return path.join(homeDir(), '.mando');
}

export function resolvePortFileName(): string {
  if (isPreview()) return 'daemon.port';
  if (isDev()) return 'daemon-dev.port';
  return 'daemon.port';
}

/** Resolve cargo target dir -- respects env overrides, then walks upward to
 * find rust/target. Shared by `cliSourcePath` below and the daemon binary's
 * dev-mode source path in `runtime/launchdPaths.ts`. */
export function cargoTargetDir(): string {
  const override = process.env.MANDO_RUST_TARGET_DIR || process.env.CARGO_TARGET_DIR;
  if (override) {
    return path.isAbsolute(override) ? override : path.resolve(process.cwd(), override);
  }

  let dir = path.resolve(__dirname);
  for (let i = 0; i < 8; i++) {
    for (const candidate of [
      path.join(dir, 'rust', 'target', 'debug', 'mando-gw'),
      path.join(dir, 'target', 'debug', 'mando-gw'),
    ]) {
      if (fs.existsSync(candidate)) {
        return path.dirname(candidate);
      }
    }
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }

  return path.resolve(__dirname, '../../../../rust/target/debug');
}

/** Source path of the bundled `mando` CLI: the packaged app's
 * `resourcesPath` in production, or the cargo build output in dev. This is
 * the same binary `copyCliBinary()` stages to `cliInstallPath()` -- but
 * auto-update refreshes the app bundle without re-running that staging
 * step, so `cliInstallPath()` can go stale after an update. Callers that
 * must always match the running app's version (e.g.
 * `codex/service/codexAppCli.ts`) spawn this path directly instead. */
export function cliSourcePath(): string {
  if (app.isPackaged) return path.join(process.resourcesPath!, 'mando');
  return path.join(cargoTargetDir(), 'mando');
}

export function generateDaemonPlist(dataDir: string): string {
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
