import type { QueryClient } from '@tanstack/react-query';

export type DaemonSyncMode =
  | 'sse-patched'
  | 'sse-invalidated'
  | 'mutation-invalidated'
  | 'polling'
  | 'manual';

export type DaemonResyncReason =
  | 'snapshot-error'
  | 'explicit-resync'
  | 'unexpected-event'
  | 'reconnect-catchup';

export function daemonSyncMeta(mode: DaemonSyncMode, detail?: string) {
  return { daemonSync: { mode, detail } } as const;
}

export function invalidateAllDaemonQueries(client: QueryClient, _reason: DaemonResyncReason): void {
  void client.invalidateQueries();
}
