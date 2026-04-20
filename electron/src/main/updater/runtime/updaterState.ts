import type { PendingUpdate } from '#main/updater/types/updater';

interface UpdaterRuntimeSnapshot {
  pendingUpdate: PendingUpdate | null;
  downloading: boolean;
  checkTimer: ReturnType<typeof setTimeout> | null;
  checkInterval: ReturnType<typeof setInterval> | null;
}

export function createUpdaterRuntimeState() {
  const snapshot: UpdaterRuntimeSnapshot = {
    pendingUpdate: null,
    downloading: false,
    checkTimer: null,
    checkInterval: null,
  };

  return {
    getPending(): PendingUpdate | null {
      return snapshot.pendingUpdate;
    },
    setPending(update: PendingUpdate | null): void {
      snapshot.pendingUpdate = update;
    },
    isDownloading(): boolean {
      return snapshot.downloading;
    },
    setDownloading(downloading: boolean): void {
      snapshot.downloading = downloading;
    },
    setCheckTimer(timer: ReturnType<typeof setTimeout>): void {
      snapshot.checkTimer = timer;
    },
    setCheckInterval(timer: ReturnType<typeof setInterval>): void {
      snapshot.checkInterval = timer;
    },
    clearTimers(): void {
      if (snapshot.checkTimer) clearTimeout(snapshot.checkTimer);
      if (snapshot.checkInterval) clearInterval(snapshot.checkInterval);
      snapshot.checkTimer = null;
      snapshot.checkInterval = null;
    },
  };
}
