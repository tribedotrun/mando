import { app } from 'electron';
import { updateDaemonBinary } from '#main/global/runtime/launchd';
import { announceUiUpdating } from '#main/global/runtime/uiLifecycle';
import log from '#main/global/providers/logger';
import { getDataDir } from '#main/global/config/lifecycle';
import {
  applyStagedUpdate,
  cleanupStagedUpdateArtifacts,
  removePendingUpdateMarker,
} from '#main/updater/service/stagedUpdate';
import type { PendingUpdate } from '#main/updater/types/updater';

interface ApplyPendingUpdateOptions {
  removeMarkerBeforeSwap?: boolean;
  onSuccess?: () => void;
  onFailure?: () => void;
}

// invariant: updater apply flow is the final app-relaunch boundary and returns bare Promise<boolean> to its owner.
export async function applyPendingUpdateFlow(
  pendingUpdate: PendingUpdate,
  options?: ApplyPendingUpdateOptions,
): Promise<boolean> {
  log.info(`auto-update: applying staged update to ${pendingUpdate.version}`);
  if (options?.removeMarkerBeforeSwap) {
    removePendingUpdateMarker();
  }
  await announceUiUpdating();

  try {
    updateDaemonBinary(getDataDir(), pendingUpdate.appPath);
  } catch (err) {
    log.warn('auto-update: pre-swap daemon binary update failed (will retry on relaunch)', err);
  }

  try {
    applyStagedUpdate(pendingUpdate.appPath);
    if (!options?.removeMarkerBeforeSwap) {
      removePendingUpdateMarker();
    }
    options?.onSuccess?.();
    app.relaunch();
    app.exit(0);
    return true;
  } catch (err) {
    log.error('auto-update: failed to apply staged update', err);
    cleanupStagedUpdateArtifacts();
    options?.onFailure?.();
    return false;
  }
}
