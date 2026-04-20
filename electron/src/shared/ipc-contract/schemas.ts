// Zod schemas for every IPC payload. Hand-authored because IPC is Electron-internal
// (no Rust source). Re-uses daemon-contract schemas where applicable.

import { z } from 'zod';
import { parseConfigJsonText } from '#shared/daemon-contract/json';
import {
  notificationKindSchema,
  notificationPayloadSchema as wireNotificationPayloadSchema,
} from '#shared/daemon-contract/schemas';

export const updateChannelSchema = z.enum(['stable', 'beta']);

export const devGitInfoResultSchema = z.object({
  branch: z.string(),
  commit: z.string(),
  worktree: z.string().nullable(),
  slot: z.string().nullable(),
});

export const appInfoResultSchema = z.object({
  appVersion: z.string(),
  stack: z.array(z.object({ name: z.string(), version: z.string() })),
});

export const checkClaudeCodeResultSchema = z.object({
  installed: z.boolean(),
  version: z.string().nullable(),
  works: z.boolean(),
});

export const telegramValidateResultSchema = z.object({
  valid: z.boolean(),
  botName: z.string().optional(),
  botUsername: z.string().optional(),
  error: z.string().optional(),
});

function validateConfigJsonString(value: string, ctx: z.RefinementCtx): void {
  const parsed = parseConfigJsonText(value, 'ipc:config-json-string');
  if (parsed.isOk()) return;
  for (const issue of parsed.error.issues) {
    ctx.addIssue({
      code: 'custom',
      path: issue.path,
      message: issue.message,
    });
  }
}

// Renderer hands main a JSON config string during onboarding. The IPC contract
// validates that the string parses into a real mando config before the handler
// writes any local file or forwards it to the daemon.
export const configJsonStringSchema = z.string().superRefine(validateConfigJsonString);
export const setupConfigPayloadSchema = configJsonStringSchema;

export const setupCompleteResultSchema = z.object({
  ok: z.boolean(),
  daemonNotified: z.boolean(),
  launchdInstalled: z.boolean(),
  error: z.string().optional(),
});

export const setupProgressPayloadSchema = z.string();

export const shortcutActionSchema = z.string();

export const pendingUpdateInfoSchema = z.object({
  version: z.string(),
  notes: z.string(),
});

export const updateReadyPayloadSchema = pendingUpdateInfoSchema;

export const updateCheckDonePayloadSchema = z.object({
  found: z.boolean(),
});

// Notifications carry the wire NotificationPayload; we re-export it so the IPC
// contract has a stable identity even if the wire shape evolves.
export const notificationPayloadSchema = wireNotificationPayloadSchema;

export const notificationClickPayloadSchema = z.object({
  kind: z.lazy(() => notificationKindSchema),
  item_id: z.string().optional(),
});
