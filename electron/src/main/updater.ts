/**
 * DIY auto-updater — bypasses Squirrel.Mac entirely.
 *
 * Downloads the update ZIP ourselves, extracts it, and swaps the .app bundle
 * via rename (mv). No ShipIt, no admin prompt.
 *
 * Channels:
 *   stable — default, receives only full releases
 *   beta   — opt-in via Settings, receives prereleases too
 *   alpha  — team-only, set via MANDO_UPDATE_CHANNEL + MANDO_ALPHA_TOKEN in config.json env
 *
 * Update flow:
 *   1. Periodic check → fetch feed from CF Worker
 *   2. If newer version → download ZIP to staging dir
 *   3. Extract → staging/Mando.app
 *   4. Send 'update-ready' IPC to renderer
 *   5a. User clicks "Update" → swap .app bundle, relaunch
 *   5b. User ignores → next app launch detects staged update, swaps, relaunches
 */
import { app, BrowserWindow } from 'electron';
import {
  readFileSync,
  writeFileSync,
  mkdirSync,
  existsSync,
  renameSync,
  rmSync,
  readdirSync,
  createWriteStream,
} from 'fs';
import path from 'path';
import https from 'https';
import { execSync } from 'child_process';
import { handleTrusted } from '#main/ipc-security';
import { getConfigPath } from '#main/daemon';
import log from '#main/logger';

const UPDATE_CHECK_INTERVAL_MS = 30 * 60 * 1000;
const INITIAL_CHECK_DELAY_MS = 10 * 1000;
const UPDATE_SERVER = 'https://mando-update.gm-e6e.workers.dev';
const MAX_REDIRECTS = 5;

type UpdateChannel = 'stable' | 'beta' | 'alpha';

interface FeedResponse {
  url: string;
  name: string;
  notes: string;
  pub_date: string;
}

interface PendingUpdate {
  version: string;
  notes: string;
  appPath: string;
}

let pendingUpdate: PendingUpdate | null = null;
let downloading = false;

/** Extract Node.js error code (e.g. 'ENOENT') or empty string. */
function errCode(err: unknown): string {
  return err instanceof Error && 'code' in err ? ((err as NodeJS.ErrnoException).code ?? '') : '';
}

// Paths

function getStagingDir(): string {
  return path.join(app.getPath('userData'), 'updates');
}

function getPendingPath(): string {
  return path.join(getStagingDir(), 'pending.json');
}

function getChannelConfigPath(): string {
  return path.join(app.getPath('userData'), 'update-channel.json');
}

function getAppBundlePath(): string {
  // process.execPath = /Applications/Mando.app/Contents/MacOS/Mando
  return path.resolve(process.execPath, '..', '..', '..');
}

// Config.json helpers — read env section for alpha token and channel override.

function readConfigEnv(): Record<string, string> {
  try {
    const raw = readFileSync(getConfigPath(), 'utf-8');
    const config = JSON.parse(raw) as { env?: Record<string, string> };
    return config.env ?? {};
  } catch {
    return {};
  }
}

// Channel persistence

function readChannel(): UpdateChannel {
  const cfgChannel = readConfigEnv().MANDO_UPDATE_CHANNEL;
  if (cfgChannel === 'alpha' || cfgChannel === 'beta') return cfgChannel;

  try {
    const raw = readFileSync(getChannelConfigPath(), 'utf-8');
    const parsed = JSON.parse(raw) as { channel?: string };
    if (parsed.channel === 'beta') return parsed.channel;
  } catch (err) {
    const code = errCode(err);
    if (code !== 'ENOENT') {
      log.warn('auto-update: failed to read channel config, defaulting to stable', err);
    }
  }
  return 'stable';
}

function writeChannel(channel: UpdateChannel): void {
  const configPath = getChannelConfigPath();
  mkdirSync(path.dirname(configPath), { recursive: true });
  writeFileSync(configPath, JSON.stringify({ channel }), 'utf-8');
}

function getAlphaToken(): string | undefined {
  return readConfigEnv().MANDO_ALPHA_TOKEN || undefined;
}

// Feed

function buildFeedUrl(): string {
  const arch = process.arch;
  const version = app.getVersion();
  const channel = readChannel();
  const channelParam = channel !== 'stable' ? `?channel=${channel}` : '';
  return `${UPDATE_SERVER}/update/darwin/${arch}/${version}${channelParam}`;
}

function buildFeedHeaders(): Record<string, string> {
  const headers: Record<string, string> = {};
  if (readChannel() === 'alpha') {
    const token = getAlphaToken();
    if (!token) {
      log.warn('auto-update: alpha channel requires MANDO_ALPHA_TOKEN');
    } else {
      headers['Authorization'] = `Bearer ${token}`;
    }
  }
  return headers;
}

function fetchFeed(): Promise<FeedResponse | null> {
  return new Promise((resolve) => {
    const url = buildFeedUrl();
    const headers = buildFeedHeaders();
    log.info(`auto-update: checking ${url}`);

    const req = https.get(url, { headers }, (res) => {
      if (res.statusCode === 204) {
        log.info('auto-update: up to date');
        resolve(null);
        return;
      }
      if (res.statusCode !== 200) {
        log.warn(`auto-update: feed returned ${res.statusCode}`);
        res.resume();
        resolve(null);
        return;
      }
      let body = '';
      res.on('data', (chunk: Buffer) => {
        body += chunk.toString();
      });
      res.on('end', () => {
        try {
          resolve(JSON.parse(body) as FeedResponse);
        } catch {
          log.error('auto-update: invalid feed JSON');
          resolve(null);
        }
      });
    });
    req.on('error', (err) => {
      log.error('auto-update: feed fetch error', err.message);
      resolve(null);
    });
  });
}

// Download

function downloadFile(url: string, dest: string, redirectsLeft = MAX_REDIRECTS): Promise<void> {
  return new Promise((resolve, reject) => {
    if (!url.startsWith('https://')) {
      reject(new Error(`Refusing non-HTTPS download URL: ${url.substring(0, 80)}`));
      return;
    }

    const file = createWriteStream(dest);

    const request = https.get(url, (res) => {
      if (res.statusCode === 301 || res.statusCode === 302) {
        const location = res.headers.location;
        if (!location) {
          reject(new Error('Redirect with no location'));
          return;
        }
        if (redirectsLeft <= 0) {
          reject(new Error('Too many redirects'));
          return;
        }
        file.close();
        downloadFile(location, dest, redirectsLeft - 1).then(resolve, reject);
        return;
      }
      if (res.statusCode !== 200) {
        file.close();
        reject(new Error(`Download failed: HTTP ${res.statusCode}`));
        return;
      }
      res.pipe(file);
      file.on('finish', () => {
        file.close();
        resolve();
      });
    });
    request.on('error', (err) => {
      file.close();
      reject(err);
    });
  });
}

// Extract + stage

function extractAndStage(zipPath: string): string {
  const stagingDir = getStagingDir();
  const extractDir = path.join(stagingDir, 'extract');

  // Clean previous extraction
  if (existsSync(extractDir)) rmSync(extractDir, { recursive: true });
  mkdirSync(extractDir, { recursive: true });

  // ditto -xk preserves code signatures and resource forks
  execSync(`ditto -xk "${zipPath}" "${extractDir}"`, { timeout: 120_000 });

  // Find the .app bundle in the extracted contents
  const entries = readdirSync(extractDir);
  const appEntry = entries.find((e) => e.endsWith('.app'));
  if (appEntry) return path.join(extractDir, appEntry);

  // Might be nested in a directory (e.g., Mando-darwin-arm64/Mando.app)
  for (const entry of entries) {
    const nested = path.join(extractDir, entry);
    const sub = readdirSync(nested);
    const nestedApp = sub.find((e) => e.endsWith('.app'));
    if (nestedApp) return path.join(nested, nestedApp);
  }

  throw new Error('No .app bundle found in ZIP');
}

// Code signature verification

function verifyCodeSignature(appPath: string): void {
  execSync(`codesign --verify --deep --strict "${appPath}"`, { timeout: 30_000 });
}

// Apply update (swap .app bundle)

function applyUpdate(newAppPath: string): void {
  const currentApp = getAppBundlePath();
  const stagingDir = getStagingDir();
  const oldAppPath = path.join(stagingDir, 'Mando-old.app');

  // Verify the new app has a valid code signature before swapping
  verifyCodeSignature(newAppPath);

  log.info(`auto-update: swapping ${currentApp} → ${newAppPath}`);

  // Clean up any previous old app
  if (existsSync(oldAppPath)) rmSync(oldAppPath, { recursive: true });

  try {
    renameSync(currentApp, oldAppPath);
    try {
      renameSync(newAppPath, currentApp);
    } catch (innerErr) {
      // Rollback: restore the original app before propagating
      log.error('auto-update: second rename failed, rolling back');
      renameSync(oldAppPath, currentApp);
      throw innerErr;
    }
  } catch (err: unknown) {
    const code = errCode(err);
    if (code !== 'EPERM' && code !== 'EACCES') throw err;

    // App is root-owned (ShipIt damage from a previous Squirrel update).
    // One-time admin prompt to remove the old bundle, then place the new one.
    // Can't chown inside a signed bundle (Gatekeeper blocks it), so we rm + mv.
    if (!currentApp.endsWith('.app') || !currentApp.startsWith('/Applications/')) {
      throw new Error(`auto-update: refusing admin rm on unexpected path: ${currentApp}`, {
        cause: err,
      });
    }
    log.warn('auto-update: permission denied, removing old app with admin privileges');
    execSync(
      `osascript -e 'do shell script "rm -rf \\"${currentApp}\\"" with administrator privileges'`,
      { timeout: 60_000 },
    );
    renameSync(newAppPath, currentApp);
  }

  log.info('auto-update: swap complete');
}

function cleanupAfterUpdate(): void {
  if (downloading) return; // don't clean up while a download is in progress

  const stagingDir = getStagingDir();
  const pendingPath = getPendingPath();
  if (existsSync(pendingPath)) rmSync(pendingPath);

  const oldAppPath = path.join(stagingDir, 'Mando-old.app');
  if (existsSync(oldAppPath)) {
    rmSync(oldAppPath, { recursive: true });
    log.info('auto-update: cleaned up old app');
  }

  const extractDir = path.join(stagingDir, 'extract');
  if (existsSync(extractDir)) {
    rmSync(extractDir, { recursive: true });
  }

  const zipPath = path.join(stagingDir, 'update.zip');
  if (existsSync(zipPath)) rmSync(zipPath);
}

// Staged update: apply on next launch

function writePending(update: PendingUpdate): void {
  const pendingPath = getPendingPath();
  mkdirSync(path.dirname(pendingPath), { recursive: true });
  // Atomic write: write to temp file, then rename
  const tmpPath = pendingPath + '.tmp';
  writeFileSync(tmpPath, JSON.stringify(update), 'utf-8');
  renameSync(tmpPath, pendingPath);
}

function readPending(): PendingUpdate | null {
  const pendingPath = getPendingPath();
  try {
    const raw = readFileSync(pendingPath, 'utf-8');
    return JSON.parse(raw) as PendingUpdate;
  } catch (err) {
    const code = errCode(err);
    if (code !== 'ENOENT') {
      log.warn('auto-update: failed to read pending update marker', err);
    }
    return null;
  }
}

/** Called early in app startup — before window creation. */
export function applyPendingUpdateIfAny(): boolean {
  const staged = readPending();
  if (!staged || !existsSync(staged.appPath)) {
    cleanupAfterUpdate();
    return false;
  }

  log.info(`auto-update: applying staged update to ${staged.version}`);

  // Delete the marker FIRST to prevent relaunch loops
  rmSync(getPendingPath());

  try {
    applyUpdate(staged.appPath);
    app.relaunch();
    app.exit(0);
    return true; // unreachable, but signals to caller
  } catch (err) {
    log.error('auto-update: failed to apply staged update', err);
    cleanupAfterUpdate();
    return false;
  }
}

// Check + download flow

async function checkAndDownload(): Promise<void> {
  if (downloading) return;
  if (pendingUpdate) return; // already have one ready

  downloading = true;

  const feed = await fetchFeed();
  if (!feed) {
    downloading = false;
    return;
  }

  log.info(`auto-update: update available: ${feed.name}`);

  const stagingDir = getStagingDir();
  mkdirSync(stagingDir, { recursive: true });
  const zipPath = path.join(stagingDir, 'update.zip');

  try {
    log.info(`auto-update: downloading from ${feed.url.substring(0, 80)}...`);
    await downloadFile(feed.url, zipPath);
    log.info('auto-update: download complete, extracting...');

    const appPath = extractAndStage(zipPath);
    log.info(`auto-update: extracted to ${appPath}`);

    pendingUpdate = { version: feed.name, notes: feed.notes, appPath };
    writePending(pendingUpdate);

    // Notify renderer
    const windows = BrowserWindow.getAllWindows();
    for (const win of windows) {
      win.webContents.send('update-ready', { version: feed.name, notes: feed.notes });
    }

    log.info(`auto-update: v${feed.name} ready to install`);
  } catch (err) {
    log.error('auto-update: download/extract failed', err);
    if (existsSync(zipPath)) rmSync(zipPath);
    const extractDir = path.join(stagingDir, 'extract');
    if (existsSync(extractDir)) rmSync(extractDir, { recursive: true });
  } finally {
    downloading = false;
  }
}

// Public API

export function setupAutoUpdate(): void {
  handleTrusted('updates:install', () => {
    if (!app.isPackaged) {
      log.info('auto-update: install requested in dev mode — ignoring');
      return;
    }
    if (!pendingUpdate) {
      log.warn('auto-update: install requested but no update pending');
      return;
    }
    log.info(`auto-update: user requested install of v${pendingUpdate.version}`);
    try {
      applyUpdate(pendingUpdate.appPath);
      rmSync(getPendingPath(), { force: true });
      app.relaunch();
      app.exit(0);
    } catch (err) {
      log.error('auto-update: install failed', err);
      cleanupAfterUpdate();
      pendingUpdate = null;
    }
  });

  handleTrusted('updates:check', () => {
    if (!app.isPackaged) {
      log.info('auto-update: manual check requested in dev mode — ignoring');
      return;
    }
    log.info('auto-update: manual check triggered');
    return checkAndDownload();
  });

  handleTrusted('updates:app-version', () => app.getVersion());
  handleTrusted('updates:pending', () => {
    if (pendingUpdate) return { version: pendingUpdate.version, notes: pendingUpdate.notes };
    return null;
  });
  handleTrusted('updates:get-channel', () => readChannel());

  handleTrusted('updates:set-channel', (_: unknown, channel: string) => {
    if (channel !== 'stable' && channel !== 'beta') return;
    writeChannel(channel);
    log.info(`auto-update: channel changed to ${channel}`);
    if (!app.isPackaged) return;
    if (pendingUpdate && !downloading) {
      cleanupAfterUpdate();
      pendingUpdate = null;
    }
    return checkAndDownload();
  });

  if (!app.isPackaged) {
    log.info('auto-update: skipping background updater in dev mode');
    return;
  }

  // Schedule periodic checks
  setTimeout(() => checkAndDownload(), INITIAL_CHECK_DELAY_MS);
  setInterval(() => checkAndDownload(), UPDATE_CHECK_INTERVAL_MS);
}
