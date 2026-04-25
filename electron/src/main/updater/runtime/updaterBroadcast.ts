import { BrowserWindow } from 'electron';
import { sendChannel } from '#main/global/runtime/ipcSecurity';

export type UpdateBroadcastChannel =
  | 'update-ready'
  | 'update-checking'
  | 'update-no-update'
  | 'update-check-error'
  | 'update-check-done';

export function broadcastToWindows(
  channel: 'update-ready',
  payload: { version: string; notes: string },
): void;
export function broadcastToWindows(channel: 'update-check-done', payload: { found: boolean }): void;
export function broadcastToWindows(
  channel: Exclude<UpdateBroadcastChannel, 'update-ready' | 'update-check-done'>,
): void;
export function broadcastToWindows(channel: UpdateBroadcastChannel, payload?: unknown): void {
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
