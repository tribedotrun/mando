/**
 * DIY auto-updater -- download, stage, and apply a signed app bundle without
 * Squirrel.Mac. This owner coordinates the explicit updater state machine while
 * feed, channel, and filesystem concerns live in dedicated service modules.
 */
import { app, BrowserWindow } from 'electron';
import { handleChannel, sendChannel } from '#main/global/runtime/ipcSecurity';
import { readAppPackageVersion } from '#main/global/runtime/appPackage';
import { updateDaemonBinary } from '#main/global/runtime/launchd';
import { announceUiUpdating } from '#main/global/runtime/uiLifecycle';
import log from '#main/global/providers/logger';
import { getDataDir } from '#main/global/config/lifecycle';
import { UPDATE_CHECK_INTERVAL_MS, INITIAL_CHECK_DELAY_MS } from '#main/updater/config/updater';
import { createUpdaterRuntimeState } from '#main/updater/runtime/updaterState';
import { readChannel, writeChannel } from '#main/updater/service/channelConfig';
import { fetchFeed, downloadFile } from '#main/updater/service/feedClient';
import {
  applyStagedUpdate,
  ensureStagingDir,
  cleanupStagedUpdateArtifacts,
  downloadPath,
  extractAndStage,
  readPendingUpdate,
  removePendingUpdateMarker,
  stagingAppExists,
  writePendingUpdate,
} from '#main/updater/service/stagedUpdate';

const runtime = createUpdaterRuntimeState();

type UpdateBroadcastChannel =
  | 'update-ready'
  | 'update-checking'
  | 'update-no-update'
  | 'update-check-error'
  | 'update-check-done';

function broadcastToWindows(
  channel: 'update-ready',
  payload: { version: string; notes: string },
): void;
function broadcastToWindows(channel: 'update-check-done', payload: { found: boolean }): void;
function broadcastToWindows(
  channel: Exclude<UpdateBroadcastChannel, 'update-ready' | 'update-check-done'>,
): void;
function broadcastToWindows(channel: UpdateBroadcastChannel, payload?: unknown): void {
  for (const window of BrowserWindow.getAllWindows()) {
    if (channel === 'update-ready') {
      sendChannel(window.webContents, channel, payload as { version: string; notes: string });
      continue;
    }
    if (channel === 'update-check-done') {
      sendChannel(window.webContents, channel, payload as { found: boolean });
      continue;
    }
    sendChannel(window.webContents, channel);
  }
}

async function stageUpdate(feed: { url: string; name: string; notes: string }) {
  ensureStagingDir();
  const zipPath = downloadPath();
  log.info(`auto-update: downloading from ${feed.url.substring(0, 80)}...`);
  await downloadFile(feed.url, zipPath);
  log.info('auto-update: download complete, extracting...');

  const appPath = extractAndStage(zipPath);
  log.info(`auto-update: extracted to ${appPath}`);

  const pendingUpdate = { version: feed.name, notes: feed.notes, appPath };
  writePendingUpdate(pendingUpdate);
  return pendingUpdate;
}

async function checkAndDownload() {
  if (runtime.isDownloading() || runtime.getPending()) return;

  runtime.setDownloading(true);
  broadcastToWindows('update-checking');

  const result: Awaited<ReturnType<typeof fetchFeed>> = await fetchFeed();
  if (result.kind !== 'update') {
    runtime.setDownloading(false);
    broadcastToWindows(result.kind === 'up-to-date' ? 'update-no-update' : 'update-check-error');
    return;
  }

  log.info(`auto-update: update available: ${result.feed.name}`);

  try {
    const pendingUpdate = await stageUpdate(result.feed);
    runtime.setPending(pendingUpdate);
    broadcastToWindows('update-ready', {
      version: pendingUpdate.version,
      notes: pendingUpdate.notes,
    });
    broadcastToWindows('update-check-done', { found: true });
    log.info(`auto-update: v${pendingUpdate.version} ready to install`);
  } catch (err) {
    log.error('auto-update: download/extract failed', err);
    cleanupStagedUpdateArtifacts();
    broadcastToWindows('update-check-error');
  } finally {
    runtime.setDownloading(false);
  }
}

export async function applyPendingUpdateIfAny() {
  const staged = readPendingUpdate();
  if (!staged || !stagingAppExists(staged.appPath)) {
    cleanupStagedUpdateArtifacts();
    return false;
  }

  log.info(`auto-update: applying staged update to ${staged.version}`);
  removePendingUpdateMarker();
  await announceUiUpdating();

  try {
    updateDaemonBinary(getDataDir(), staged.appPath);
  } catch (err) {
    log.warn('auto-update: pre-swap daemon binary update failed (will retry on relaunch)', err);
  }

  try {
    applyStagedUpdate(staged.appPath);
    app.relaunch();
    app.exit(0);
    return true;
  } catch (err) {
    log.error('auto-update: failed to apply staged update', err);
    cleanupStagedUpdateArtifacts();
    return false;
  }
}

export function setupAutoUpdate(): void {
  handleChannel('updates:install', async () => {
    if (!app.isPackaged) {
      log.info('auto-update: install requested in dev mode — ignoring');
      return;
    }

    const pendingUpdate = runtime.getPending();
    if (!pendingUpdate) {
      log.warn('auto-update: install requested but no update pending');
      return;
    }

    log.info(`auto-update: user requested install of v${pendingUpdate.version}`);
    try {
      await announceUiUpdating();
      try {
        updateDaemonBinary(getDataDir(), pendingUpdate.appPath);
      } catch (err) {
        log.warn('auto-update: pre-swap daemon binary update failed (will retry on relaunch)', err);
      }
      applyStagedUpdate(pendingUpdate.appPath);
      removePendingUpdateMarker();
      runtime.setPending(null);
      app.relaunch();
      app.exit(0);
    } catch (err) {
      log.error('auto-update: install failed', err);
      cleanupStagedUpdateArtifacts();
      runtime.setPending(null);
      throw err;
    }
  });

  handleChannel('updates:check', async () => {
    if (!app.isPackaged) {
      log.info('auto-update: manual check requested in dev mode');
      broadcastToWindows('update-no-update');
      return;
    }
    log.info('auto-update: manual check triggered');
    await checkAndDownload();
  });

  handleChannel('updates:app-version', () => readAppPackageVersion() ?? app.getVersion());
  handleChannel('updates:pending', () => {
    const pendingUpdate = runtime.getPending();
    return pendingUpdate ? { version: pendingUpdate.version, notes: pendingUpdate.notes } : null;
  });
  handleChannel('updates:get-channel', () => readChannel());

  handleChannel('updates:set-channel', async (_event, channel) => {
    writeChannel(channel);
    log.info(`auto-update: channel changed to ${channel}`);
    if (!app.isPackaged) return;
    if (runtime.getPending() && !runtime.isDownloading()) {
      cleanupStagedUpdateArtifacts();
      runtime.setPending(null);
    }
    await checkAndDownload();
  });

  if (!app.isPackaged) {
    log.info('auto-update: skipping background updater in dev mode');
    return;
  }

  runtime.setCheckTimer(
    setTimeout(
      () => void checkAndDownload().catch((err) => log.error('Update check failed', err)),
      INITIAL_CHECK_DELAY_MS,
    ),
  );
  runtime.setCheckInterval(
    setInterval(
      () => void checkAndDownload().catch((err) => log.error('Update check failed', err)),
      UPDATE_CHECK_INTERVAL_MS,
    ),
  );
}

export function cleanupAutoUpdate(): void {
  runtime.clearTimers();
}
