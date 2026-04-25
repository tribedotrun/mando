// Central IPC channel registry. Single source of truth for the renderer<->main boundary.
// MandoAPI in preload/providers/ipc.ts is derived from `inferIpcApi<typeof channels>`.

import { z } from 'zod';

import {
  appInfoResultSchema,
  checkClaudeCodeResultSchema,
  configJsonStringSchema,
  devGitInfoResultSchema,
  notificationClickPayloadSchema,
  notificationPayloadSchema,
  pendingUpdateInfoSchema,
  setupCompleteResultSchema,
  setupConfigPayloadSchema,
  setupProgressPayloadSchema,
  shortcutActionSchema,
  telegramValidateResultSchema,
  updateChannelSchema,
  updateCheckDonePayloadSchema,
  updateReadyPayloadSchema,
} from './schemas';

// Internal helpers that capture both Zod schema (runtime) and inferred TS type (compile).
type Schema = z.ZodType<unknown>;

export interface InvokeChannelDef<A extends Schema | null, R extends Schema | null> {
  kind: 'invoke';
  args: A;
  result: R;
}

export interface SubscribeChannelDef<P extends Schema | null> {
  kind: 'subscribe';
  payload: P;
}

export type ChannelDef =
  | InvokeChannelDef<Schema | null, Schema | null>
  | SubscribeChannelDef<Schema | null>;

function invoke<A extends Schema | null, R extends Schema | null>(
  args: A,
  result: R,
): InvokeChannelDef<A, R> {
  return { kind: 'invoke', args, result };
}

function subscribe<P extends Schema | null>(payload: P): SubscribeChannelDef<P> {
  return { kind: 'subscribe', payload };
}

// All channels live here. Key is the runtime channel name. The renderer uses the
// preload-exposed `mandoAPI.<methodName>(...)`; the channel name is wired in
// `expose.ts` (or via auto-derivation if we add that later).
export const channels = {
  // App / lifecycle
  'get-app-mode': invoke(null, z.string()),
  'get-dev-git-info': invoke(null, devGitInfoResultSchema),
  'get-gateway-url': invoke(null, z.string().nullable()),
  'get-app-info': invoke(null, appInfoResultSchema),
  'restart-daemon': invoke(null, z.boolean()),
  'toggle-devtools': invoke(null, z.void()),

  // Onboarding
  'has-config': invoke(null, z.boolean()),
  'read-config': invoke(null, configJsonStringSchema),
  'save-config-local': invoke(configJsonStringSchema, z.boolean()),
  'setup-complete': invoke(setupConfigPayloadSchema, setupCompleteResultSchema),
  'check-claude-code': invoke(null, checkClaudeCodeResultSchema),
  'validate-telegram-token': invoke(z.string(), telegramValidateResultSchema),

  // Updater
  'updates:install': invoke(null, z.void()),
  'updates:check': invoke(null, z.void()),
  'updates:app-version': invoke(null, z.string()),
  'updates:pending': invoke(null, pendingUpdateInfoSchema.nullable()),
  'updates:get-channel': invoke(null, updateChannelSchema),
  'updates:set-channel': invoke(updateChannelSchema, z.void()),

  // Native shell
  'select-directory': invoke(null, z.string().nullable()),
  'set-login-item': invoke(z.boolean(), z.void()),
  'open-logs-folder': invoke(null, z.void()),
  'open-data-dir': invoke(null, z.void()),
  'open-config-file': invoke(null, z.void()),
  'open-in-finder': invoke(z.string(), z.void()),
  'open-in-cursor': invoke(z.string(), z.void()),
  'terminal:open-external-url': invoke(z.string(), z.void()),
  'terminal:resolve-local-path': invoke(z.tuple([z.string(), z.string()]), z.string().nullable()),
  'terminal:open-local-path': invoke(z.string(), z.void()),

  // Notifications: renderer pushes a payload to main; main pushes click events back.
  'show-notification': subscribe(notificationPayloadSchema),
  'notification-click': subscribe(notificationClickPayloadSchema),

  // Push channels (main -> renderer)
  'setup-progress': subscribe(setupProgressPayloadSchema),
  shortcut: subscribe(shortcutActionSchema),
  'update-ready': subscribe(updateReadyPayloadSchema),
  'update-checking': subscribe(z.void()),
  'update-no-update': subscribe(z.void()),
  'update-check-error': subscribe(z.void()),
  'update-check-done': subscribe(updateCheckDonePayloadSchema),
} as const satisfies Record<string, ChannelDef>;

export type Channels = typeof channels;
export type ChannelName = keyof Channels;

// Helper inference types
export type ArgsOf<K extends ChannelName> =
  Channels[K] extends InvokeChannelDef<infer A, Schema | null>
    ? A extends z.ZodType<infer T>
      ? T
      : void
    : never;
export type ResultOf<K extends ChannelName> =
  Channels[K] extends InvokeChannelDef<Schema | null, infer R>
    ? R extends z.ZodType<infer T>
      ? T
      : void
    : never;
export type PayloadOf<K extends ChannelName> =
  Channels[K] extends SubscribeChannelDef<infer P>
    ? P extends z.ZodType<infer T>
      ? T
      : void
    : never;
