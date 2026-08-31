interface QuitForPendingUpdateDeps {
  announceUiUpdating: () => Promise<unknown>;
  relaunch: () => void;
  exit: (code: number) => void;
}

// invariant: the sidebar Update click path must announce `Updating` to the
// daemon and then quit Electron without tearing the daemon down in-process.
// The pending marker is already on disk from `stageUpdate()`; the next
// process boot's `applyPendingUpdateIfAny()` performs the daemon bootout +
// `.app` swap before any BrowserWindow is created.
export async function quitForPendingUpdate(deps: QuitForPendingUpdateDeps): Promise<void> {
  await deps.announceUiUpdating();
  deps.relaunch();
  deps.exit(0);
}
