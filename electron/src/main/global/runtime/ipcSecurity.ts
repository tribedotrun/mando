import {
  ipcMain,
  type IpcMainEvent,
  type IpcMainInvokeEvent,
  type WebContents,
  type WebFrameMain,
} from 'electron';
import log from '#main/global/providers/logger';
import {
  frameUrl,
  isTrustedRendererUrl as isTrustedRendererUrlPure,
  isTrustedSenderFrame as isTrustedSenderFramePure,
} from '#main/global/service/ipcSecurity';
import {
  argsSchema,
  channels,
  type ChannelName,
  type ArgsOf,
  type ResultOf,
  type PayloadOf,
} from '#shared/ipc-contract';

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

// Schema-aware variants. Use these for any channel registered in `channels`. The args
// (or single payload) are parsed against the channel's Zod schema before the handler
// runs; parse failure rejects the call with a typed error.

export function handleChannel<K extends ChannelName>(
  channel: K,
  handler: (event: IpcMainInvokeEvent, args: ArgsOf<K>) => Promise<ResultOf<K>> | ResultOf<K>,
): void {
  const def = channels[channel];
  if (def.kind !== 'invoke') {
    // invariant: handleChannel is for invoke channels only; subscribe channels use onChannel
    throw new Error(`handleChannel: ${String(channel)} is not an invoke channel`);
  }
  ipcMain.handle(channel, async (event, ...rawArgs: unknown[]) => {
    if (!isTrustedSenderFrame(event.senderFrame)) {
      rejectUntrusted(channel, event.senderFrame);
    }
    const schema = argsSchema(channel);
    let args: ArgsOf<K>;
    if (schema) {
      if (rawArgs.length > 1) {
        log.warn(`[ipc] ${channel} args rejected: extra payload args`, rawArgs);
        // invariant: every invoke channel carries at most one args payload value
        throw new Error(`IPC ${channel} args rejected: extra payload args`);
      }
      const raw = rawArgs[0];
      const parsed = schema.safeParse(raw);
      if (!parsed.success) {
        log.warn(`[ipc] ${channel} args parse failed`, parsed.error.issues);
        throw new Error(`IPC ${channel} args failed schema parse`);
      }
      args = parsed.data as ArgsOf<K>;
    } else {
      if (rawArgs.length > 0) {
        log.warn(`[ipc] ${channel} args rejected: channel does not declare args`, rawArgs);
        // invariant: invoke channels without an args schema must not receive a payload value
        throw new Error(`IPC ${channel} args rejected: channel does not declare args`);
      }
      args = undefined as ArgsOf<K>;
    }
    return handler(event, args);
  });
}

export function onChannel<K extends ChannelName>(
  channel: K,
  listener: (event: IpcMainEvent, payload: PayloadOf<K>) => void,
): void {
  const def = channels[channel];
  if (def.kind !== 'subscribe') {
    // invariant: onChannel is for subscribe channels only; invoke channels use handleChannel
    throw new Error(`onChannel: ${String(channel)} is not a subscribe channel`);
  }
  ipcMain.on(channel, (event, ...rawArgs: unknown[]) => {
    if (!isTrustedSenderFrame(event.senderFrame)) {
      log.warn(
        `[ipc] dropped untrusted event for ${String(channel)}: ${frameUrl(event.senderFrame) || '(missing url)'}`,
      );
      return;
    }
    const schema = def.payload;
    let payload: PayloadOf<K>;
    if (schema) {
      if (rawArgs.length > 1) {
        log.warn(`[ipc] ${String(channel)} payload rejected: extra payload args`, rawArgs);
        return;
      }
      const raw = rawArgs[0];
      const parsed = schema.safeParse(raw);
      if (!parsed.success) {
        log.warn(`[ipc] ${String(channel)} payload parse failed`, parsed.error.issues);
        return;
      }
      payload = parsed.data as PayloadOf<K>;
    } else {
      if (rawArgs.length > 0) {
        log.warn(
          `[ipc] ${String(channel)} payload rejected: channel does not declare a payload`,
          rawArgs,
        );
        return;
      }
      payload = undefined as PayloadOf<K>;
    }
    listener(event, payload);
  });
}

type SubscribeChannelName = {
  [K in ChannelName]: (typeof channels)[K] extends { kind: 'subscribe' } ? K : never;
}[ChannelName];

export function sendChannel<K extends SubscribeChannelName>(
  target: Pick<WebContents, 'send'>,
  channel: K,
  ...args: PayloadOf<K> extends void ? [] : [PayloadOf<K>]
): void {
  const def = channels[channel];
  if (def.kind !== 'subscribe') {
    throw new Error(`sendChannel: ${String(channel)} is not a subscribe channel`);
  }
  if (!def.payload) {
    if (args.length > 0) {
      // invariant: subscribe channels without a payload schema must not receive a payload at runtime
      throw new Error(`sendChannel: ${String(channel)} does not declare a payload`);
    }
    target.send(channel);
    return;
  }

  const payload = args[0];
  const parsed = def.payload.safeParse(payload);
  if (!parsed.success) {
    log.warn(`[ipc] ${String(channel)} outbound payload parse failed`, parsed.error.issues);
    throw new Error(`IPC ${String(channel)} outbound payload failed schema parse`);
  }

  target.send(channel, parsed.data);
}
