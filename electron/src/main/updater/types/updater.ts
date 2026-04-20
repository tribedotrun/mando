import { z } from 'zod';

export type UpdateChannel = 'stable' | 'beta';

export const updateChannelSchema = z.enum(['stable', 'beta']);

export const channelConfigSchema = z.object({
  channel: updateChannelSchema.optional(),
});

// Network boundary: feed returned by the Cloudflare Worker. Constraints enforced
// because feed.url is downloaded over HTTPS and codesign-verified later.
export const feedResponseSchema = z.object({
  url: z
    .string()
    .url()
    .refine((u) => u.startsWith('https://'), 'feed url must be https'),
  name: z.string(),
  notes: z.string(),
  pub_date: z.string(),
});
export type FeedResponse = z.infer<typeof feedResponseSchema>;

// File boundary: pending-update.json on disk. appPath is later passed to
// codesign --verify and renameSync, so we require an absolute path.
export const pendingUpdateSchema = z.object({
  version: z.string(),
  notes: z.string(),
  appPath: z.string().refine((p) => p.startsWith('/'), 'appPath must be absolute'),
});
export type PendingUpdate = z.infer<typeof pendingUpdateSchema>;

export type FeedResult =
  | { kind: 'update'; feed: FeedResponse }
  | { kind: 'up-to-date' }
  | { kind: 'error' };
