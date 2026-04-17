import { contextBridge } from 'electron';
import type { MandoAPI } from '#preload/types/api';
import { ipcApi } from '#preload/providers/ipc';

/** Expose the IPC API to the renderer via contextBridge. */
export function exposeApi(): void {
  contextBridge.exposeInMainWorld('mandoAPI', ipcApi satisfies MandoAPI);
}
