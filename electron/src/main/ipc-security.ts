import type { IpcMainEvent, IpcMainInvokeEvent, WebFrameMain } from 'electron';
import { ipcMain } from 'electron';
import log from '#main/logger';

const trustedOrigins = new Set<string>();

function frameUrl(frame: WebFrameMain | null | undefined): string {
  return frame?.url ?? '';
}

function isTrustedSenderFrame(frame: WebFrameMain | null | undefined): boolean {
  if (!frame) return false;
  return isTrustedRendererUrl(frameUrl(frame));
}

export function isTrustedRendererUrl(url: string): boolean {
  if (!url) return false;
  try {
    return trustedOrigins.has(new URL(url).origin);
  } catch {
    return false;
  }
}

function rejectUntrusted(channel: string, frame: WebFrameMain | null | undefined): never {
  const url = frameUrl(frame);
  log.warn(`[ipc] rejected untrusted sender for ${channel}: ${url || '(missing url)'}`);
  throw new Error('untrusted sender');
}

export function setTrustedRendererOrigins(origins: string[]): void {
  trustedOrigins.clear();
  for (const origin of origins) {
    trustedOrigins.add(origin);
  }
  log.info(`[ipc] trusted renderer origins: ${Array.from(trustedOrigins).join(', ')}`);
}

export function handleTrusted<TArgs extends unknown[], TResult>(
  channel: string,
  listener: (event: IpcMainInvokeEvent, ...args: TArgs) => Promise<TResult> | TResult,
): void {
  ipcMain.handle(channel, (event, ...args: TArgs) => {
    if (!isTrustedSenderFrame(event.senderFrame)) {
      rejectUntrusted(channel, event.senderFrame);
    }
    return listener(event, ...args);
  });
}

export function onTrusted<TArgs extends unknown[]>(
  channel: string,
  listener: (event: IpcMainEvent, ...args: TArgs) => void,
): void {
  ipcMain.on(channel, (event, ...args: TArgs) => {
    if (!isTrustedSenderFrame(event.senderFrame)) {
      log.warn(
        `[ipc] dropped untrusted event for ${channel}: ${frameUrl(event.senderFrame) || '(missing url)'}`,
      );
      return;
    }
    listener(event, ...args);
  });
}
