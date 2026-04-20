// Public surface of the IPC contract module. Channels + their schemas are the
// single source of truth for the renderer<->main boundary.

export {
  channels,
  type ChannelName,
  type ArgsOf,
  type ResultOf,
  type PayloadOf,
} from './channels.ts';

export {
  argsSchema,
  resultSchema,
  payloadSchema,
  invoke,
  subscribe,
  send,
  type InvokeApi,
  type SubscribeApi,
  type InferIpcApi,
} from './runtime.ts';

export * from './schemas.ts';
