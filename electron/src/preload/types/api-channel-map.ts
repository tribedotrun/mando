// Compile-time drift check between MandoAPI's hand-typed surface and the
// channel registry's runtime schemas. Each MandoAPI method maps to a channel
// name; the assertions below fail to compile if either:
//   1. The channel doesn't exist, or
//   2. The MandoAPI return / arg / payload type disagrees with the channel's schema.
//
// This eliminates the "MandoAPI silently drifts from runtime behavior" hazard
// the parity audit flagged: any renamed channel or schema change forces this
// file (and therefore CI) to surface the mismatch.
//
// Coverage: every key in InvokeMap, UpdatesInvokeMap, SubscribeMap, and
// UpdatesSubscribeMap is checked via mapped types -- adding a new channel to
// any map without updating MandoAPI produces a compile error automatically.

import type { MandoAPI } from '#preload/types/api';
import type { ArgsOf, ResultOf, PayloadOf } from '#shared/ipc-contract';

/** Every invoke method on MandoAPI must map to an invoke channel. */
type InvokeMap = {
  appMode: 'get-app-mode';
  devGitInfo: 'get-dev-git-info';
  checkClaudeCode: 'check-claude-code';
  validateTelegramToken: 'validate-telegram-token';
  gatewayUrl: 'get-gateway-url';
  appInfo: 'get-app-info';
  hasConfig: 'has-config';
  readConfig: 'read-config';
  saveConfigLocal: 'save-config-local';
  setupComplete: 'setup-complete';
  restartDaemon: 'restart-daemon';
  selectDirectory: 'select-directory';
  setLoginItem: 'set-login-item';
  toggleDevTools: 'toggle-devtools';
  openExternalUrl: 'terminal:open-external-url';
  resolveLocalPath: 'terminal:resolve-local-path';
  openLocalPath: 'terminal:open-local-path';
  openInFinder: 'open-in-finder';
  openInCursor: 'open-in-cursor';
};

/** Updates sub-namespace methods. Same drift check, scoped to MandoAPI.updates. */
type UpdatesInvokeMap = {
  installUpdate: 'updates:install';
  checkForUpdates: 'updates:check';
  getPending: 'updates:pending';
  appVersion: 'updates:app-version';
  getChannel: 'updates:get-channel';
  setChannel: 'updates:set-channel';
};

/** Subscribe-channel methods on MandoAPI root. */
type SubscribeMap = {
  onSetupProgress: 'setup-progress';
  onShortcut: 'shortcut';
  onNotificationClick: 'notification-click';
};

/** Subscribe-channel methods on MandoAPI.updates. */
type UpdatesSubscribeMap = {
  onUpdateReady: 'update-ready';
  onUpdateChecking: 'update-checking';
  onUpdateNoUpdate: 'update-no-update';
  onUpdateCheckError: 'update-check-error';
  onUpdateCheckDone: 'update-check-done';
};

// --- Type-level assertion helpers --------------------------------------------

/** True iff the Promise resolves to exactly Expected (mutual subtype). */
type AssertReturn<T extends Promise<unknown>, Expected> =
  T extends Promise<infer R>
    ? R extends Expected
      ? Expected extends R
        ? true
        : never
      : never
    : never;

// --- Exhaustive mapped-type assertions ---------------------------------------
// Each mapped type iterates every key in the corresponding map, so adding a
// new channel without updating MandoAPI produces a compile error on the `true`
// assignment below. K extends keyof MandoAPI ensures the field must exist.

/**
 * Return-type parity for every root invoke method.
 * Note: resolveLocalPath arg-shape intentionally differs (preload bundles
 * two separate params into a tuple before passing to invoke); arg assertions
 * for multi-param methods are omitted by design -- see per-field section below.
 */
type InvokeReturnAssertions = {
  [K in keyof InvokeMap]: K extends keyof MandoAPI
    ? AssertReturn<ReturnType<MandoAPI[K]>, ResultOf<InvokeMap[K]>>
    : never;
};

/** Return-type parity for every updates invoke method. */
type UpdatesInvokeReturnAssertions = {
  [K in keyof UpdatesInvokeMap]: K extends keyof MandoAPI['updates']
    ? AssertReturn<ReturnType<MandoAPI['updates'][K]>, ResultOf<UpdatesInvokeMap[K]>>
    : never;
};

/**
 * Payload parity for root subscribe methods.
 * Pattern: Parameters<Parameters<MandoAPI[K]>[0]>[0] is the callback's first arg.
 * For void-payload channels (z.void()) the callback takes no args, so this is
 * undefined, and undefined extends void is true -- no special case needed.
 */
type SubscribePayloadAssertions = {
  [K in keyof SubscribeMap]: K extends keyof MandoAPI
    ? Parameters<Parameters<MandoAPI[K]>[0]>[0] extends PayloadOf<SubscribeMap[K]>
      ? true
      : never
    : never;
};

/** Payload parity for updates subscribe methods. */
type UpdatesSubscribePayloadAssertions = {
  [K in keyof UpdatesSubscribeMap]: K extends keyof MandoAPI['updates']
    ? Parameters<Parameters<MandoAPI['updates'][K]>[0]>[0] extends PayloadOf<UpdatesSubscribeMap[K]>
      ? true
      : never
    : never;
};

// --- Per-field arg assertions (single-arg invoke methods only) ---------------
// Multi-param methods (resolveLocalPath) are excluded: the preload bundles
// separate params into a tuple before calling invoke(), so the MandoAPI
// signature and the channel's ArgsOf deliberately differ.

type ArgAssertions = {
  validateTelegramTokenArg: Parameters<MandoAPI['validateTelegramToken']>[0] extends ArgsOf<
    InvokeMap['validateTelegramToken']
  >
    ? true
    : never;
  setLoginItemArg: Parameters<MandoAPI['setLoginItem']>[0] extends ArgsOf<InvokeMap['setLoginItem']>
    ? true
    : never;
  setChannelArg: Parameters<MandoAPI['updates']['setChannel']>[0] extends ArgsOf<
    UpdatesInvokeMap['setChannel']
  >
    ? true
    : never;
};

// --- Force evaluation --------------------------------------------------------
// If any mapped assertion resolves to `never`, assigning `true` to that field fails.

const _invokeReturns: InvokeReturnAssertions = {
  appMode: true,
  devGitInfo: true,
  checkClaudeCode: true,
  validateTelegramToken: true,
  gatewayUrl: true,
  appInfo: true,
  hasConfig: true,
  readConfig: true,
  saveConfigLocal: true,
  setupComplete: true,
  restartDaemon: true,
  selectDirectory: true,
  setLoginItem: true,
  toggleDevTools: true,
  openExternalUrl: true,
  resolveLocalPath: true,
  openLocalPath: true,
  openInFinder: true,
  openInCursor: true,
};

const _updatesInvokeReturns: UpdatesInvokeReturnAssertions = {
  installUpdate: true,
  checkForUpdates: true,
  getPending: true,
  appVersion: true,
  getChannel: true,
  setChannel: true,
};

const _subscribePayloads: SubscribePayloadAssertions = {
  onSetupProgress: true,
  onShortcut: true,
  onNotificationClick: true,
};

const _updatesSubscribePayloads: UpdatesSubscribePayloadAssertions = {
  onUpdateReady: true,
  onUpdateChecking: true,
  onUpdateNoUpdate: true,
  onUpdateCheckError: true,
  onUpdateCheckDone: true,
};

const _argAssertions: ArgAssertions = {
  validateTelegramTokenArg: true,
  setLoginItemArg: true,
  setChannelArg: true,
};

// Reference all assertion objects to satisfy lint without exporting more surface.
export const __apiChannelMap = {
  _invokeReturns,
  _updatesInvokeReturns,
  _subscribePayloads,
  _updatesSubscribePayloads,
  _argAssertions,
};
