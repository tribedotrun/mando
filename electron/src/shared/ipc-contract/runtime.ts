// Helpers that wire the IPC contract to ipcMain / ipcRenderer at runtime.
// Both sides parse against the same Zod schemas.

import { ipcRenderer, type IpcRendererEvent } from 'electron';
import { channels, type ChannelName, type ArgsOf, type ResultOf, type PayloadOf } from './channels';

// inferIpcApi: maps the channel registry into the renderer-facing API shape.
// MandoAPI in preload/types/api.ts is `type MandoAPI = inferIpcApi<typeof channels>`,
// eliminating the duplicate type definition that previously drifted from runtime.

export type InvokeApi = {
  [K in ChannelName as IsInvokeChannel<K> extends true ? K : never]: ArgsOf<K> extends void
    ? () => Promise<ResultOf<K>>
    : (args: ArgsOf<K>) => Promise<ResultOf<K>>;
};

export type SubscribeApi = {
  [K in ChannelName as IsSubscribeChannel<K> extends true ? K : never]: (
    cb: (payload: PayloadOf<K>) => void,
  ) => () => void;
};

export type InferIpcApi = InvokeApi & SubscribeApi;

type IsInvokeChannel<K extends ChannelName> = (typeof channels)[K] extends { kind: 'invoke' }
  ? true
  : false;
type IsSubscribeChannel<K extends ChannelName> = (typeof channels)[K] extends {
  kind: 'subscribe';
}
  ? true
  : false;
type SubscribeChannelName = {
  [K in ChannelName]: IsSubscribeChannel<K> extends true ? K : never;
}[ChannelName];

// Schema lookup helpers (runtime).
export function argsSchema<K extends ChannelName>(name: K) {
  const def = channels[name];
  return def.kind === 'invoke' ? def.args : null;
}

export function resultSchema<K extends ChannelName>(name: K) {
  const def = channels[name];
  return def.kind === 'invoke' ? def.result : null;
}

export function payloadSchema<K extends ChannelName>(name: K) {
  const def = channels[name];
  return def.kind === 'subscribe' ? def.payload : null;
}

// Preload-side runtime helpers. `invoke` parses the result against the channel's
// result schema; `subscribe` parses every payload before invoking the callback.
// Parse failure on result or payload is logged and the call resolves to a
// best-effort fallback (undefined / no callback fire) rather than corrupting the
// renderer's typed view.

export async function invoke<K extends ChannelName>(
  name: K,
  ...args: ArgsOf<K> extends void ? [] : [ArgsOf<K>]
): Promise<ResultOf<K>> {
  const def = channels[name];
  if (def.kind !== 'invoke') {
    // invariant: invoke() is only valid for invoke channels (TS enforces, runtime guards)
    throw new Error(`invoke: ${String(name)} is not an invoke channel`);
  }
  if (!def.args) {
    if (args.length > 0) {
      // invariant: invoke channels without an args schema must not receive a payload at runtime
      throw new Error(`invoke: ${String(name)} does not declare args in the IPC contract`);
    }
  } else {
    const parsedArgs = def.args.safeParse(args[0]);
    if (!parsedArgs.success) {
      // invariant: invoke-channel args must match the shared IPC contract before send
      throw new Error(
        `IPC ${String(name)} args failed schema parse: ${parsedArgs.error.issues[0]?.message ?? 'unknown'}`,
      );
    }
    args[0] = parsedArgs.data as ArgsOf<K>;
  }

  const raw: unknown = await ipcRenderer.invoke(name, ...args);
  if (!def.result) return undefined as ResultOf<K>;
  const parsed = def.result.safeParse(raw);
  if (!parsed.success) {
    // invariant: main-side handleChannel validates result before send; mismatch here is a contract drift
    throw new Error(
      `IPC ${String(name)} result failed schema parse: ${parsed.error.issues[0]?.message ?? 'unknown'}`,
    );
  }
  return parsed.data as ResultOf<K>;
}

export function subscribe<K extends ChannelName>(
  name: K,
  cb: (payload: PayloadOf<K>) => void,
): () => void {
  const def = channels[name];
  if (def.kind !== 'subscribe') {
    // invariant: subscribe() is only valid for subscribe channels
    throw new Error(`subscribe: ${String(name)} is not a subscribe channel`);
  }
  const listener = (_event: IpcRendererEvent, ...rawArgs: unknown[]) => {
    if (rawArgs.length > 1) {
      console.warn(`[ipc:subscribe] extra payload args on "${String(name)}"`, rawArgs);
    }
    if (!def.payload) {
      if (rawArgs.length > 0) {
        console.warn(`[ipc:subscribe] unexpected payload on "${String(name)}"`, rawArgs[0]);
      }
      cb(undefined as PayloadOf<K>);
      return;
    }
    const raw = rawArgs[0];
    const parsed = def.payload.safeParse(raw);
    if (parsed.success) {
      cb(parsed.data as PayloadOf<K>);
    } else {
      console.warn(
        `[ipc:subscribe] payload schema rejection on "${String(name)}"`,
        parsed.error.issues,
      );
    }
  };
  ipcRenderer.on(name, listener);
  return () => {
    ipcRenderer.removeListener(name, listener);
  };
}

// Push-only sender for renderer→main subscribe channels (e.g. show-notification).
export function send<K extends SubscribeChannelName>(
  name: K,
  ...args: PayloadOf<K> extends void ? [] : [PayloadOf<K>]
): void {
  const def = channels[name];
  if (def.kind !== 'subscribe') {
    // invariant: send() is for subscribe channels (renderer pushes payload to main)
    throw new Error(`send: ${String(name)} is not a subscribe channel`);
  }
  if (!def.payload) {
    if (args.length > 0) {
      // invariant: subscribe channels without a payload schema must not receive a payload at runtime
      throw new Error(`send: ${String(name)} does not declare a payload in the IPC contract`);
    }
    ipcRenderer.send(name);
    return;
  }
  const payload = args[0];
  const parsed = def.payload.safeParse(payload);
  if (!parsed.success) {
    // invariant: subscribe-channel payloads must match the shared IPC contract before send
    throw new Error(
      `IPC ${String(name)} payload failed schema parse: ${parsed.error.issues[0]?.message ?? 'unknown'}`,
    );
  }
  ipcRenderer.send(name, parsed.data);
}
