import { ipcMain, type IpcMainEvent, type IpcMainInvokeEvent, type WebFrameMain } from 'electron';
import log from '#main/global/providers/logger';
import {
  frameUrl,
  isTrustedRendererUrl as isTrustedRendererUrlPure,
  isTrustedSenderFrame as isTrustedSenderFramePure,
} from '#main/global/service/ipcSecurity';

const trustedOrigins = new Set<string>();

export function setTrustedRendererOrigins(origins: string[]): void {
  trustedOrigins.clear();
  for (const origin of origins) {
    trustedOrigins.add(origin);
  }
  log.info(`[ipc] trusted renderer origins: ${Array.from(trustedOrigins).join(', ')}`);
}

export function isTrustedRendererUrl(url: string): boolean {
  return isTrustedRendererUrlPure(url, trustedOrigins);
}

function isTrustedSenderFrame(frame: WebFrameMain | null | undefined): boolean {
  return isTrustedSenderFramePure(frame, trustedOrigins);
}

function rejectUntrusted(channel: string, frame: WebFrameMain | null | undefined): never {
  const url = frameUrl(frame);
  log.warn(`[ipc] rejected untrusted sender for ${channel}: ${url || '(missing url)'}`);
  throw new Error('untrusted sender');
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
