/**
 * DIY auto-updater -- download, stage, and apply a signed app bundle without
 * Squirrel.Mac. Feed, staging, install, and broadcast concerns each live in
 * dedicated helpers; this owner keeps the IPC/timer wiring thin.
 */
import { app } from 'electron';
import { handleChannel } from '#main/global/runtime/ipcSecurity';
import { readAppPackageVersion } from '#main/global/runtime/appPackage';
import log from '#main/global/providers/logger';
import { announceUiUpdating } from '#main/global/runtime/uiLifecycle';
import { UPDATE_CHECK_INTERVAL_MS, INITIAL_CHECK_DELAY_MS } from '#main/updater/config/updater';
import { createUpdaterRuntimeState } from '#main/updater/runtime/updaterState';
import { broadcastToWindows } from '#main/updater/runtime/updaterBroadcast';
import { applyPendingUpdateFlow } from '#main/updater/runtime/applyPendingUpdateFlow';
import { quitForPendingUpdate } from '#main/updater/runtime/installClickPath';
import { readChannel, writeChannel } from '#main/updater/service/channelConfig';
import { fetchFeed, downloadFile } from '#main/updater/service/feedClient';
import {
  cleanupStagedUpdateArtifacts,
  downloadPath,
  ensureStagingDir,
  extractAndStage,
  readPendingUpdate,
  stagingAppExists,
  writePendingUpdate,
} from '#main/updater/service/stagedUpdate';

const runtime = createUpdaterRuntimeState();

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

  return applyPendingUpdateFlow(staged, { removeMarkerBeforeSwap: true });
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

    // Defer the daemon bootout + .app swap to the next process boot's
    // `applyPendingUpdateIfAny()` so the renderer is gone before the daemon
    // disappears. Doing the work in-process leaves the renderer alive while
    // SSE drops, which surfaces a "Daemon disconnected — Reconnecting…"
    // banner for several seconds before the window finally closes. The
    // pending marker is already on disk from `stageUpdate()`.
    log.info(`auto-update: deferring apply of v${pendingUpdate.version} to next launch`);
    await quitForPendingUpdate({
      announceUiUpdating,
      relaunch: () => app.relaunch(),
      exit: (code) => app.exit(code),
    });
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

  const runScheduledCheck = async (): Promise<void> => {
    try {
      await checkAndDownload();
    } catch (err) {
      log.error('Update check failed', err);
    }
  };

  runtime.setCheckTimer(setTimeout(() => void runScheduledCheck(), INITIAL_CHECK_DELAY_MS));
  runtime.setCheckInterval(setInterval(() => void runScheduledCheck(), UPDATE_CHECK_INTERVAL_MS));
}

export function cleanupAutoUpdate(): void {
  runtime.clearTimers();
}
