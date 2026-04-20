import { useDevInfoQuery, type DevInfo } from '#renderer/global/repo/queries';

export type { DevInfo };

/** Load dev info (mode, port, git state) from IPC. Returns null in production. */
export function useDevInfo(): DevInfo | null {
  const { data } = useDevInfoQuery();
  return data ?? null;
}
