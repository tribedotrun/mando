import { z } from 'zod';

export const TTL_MS = 14 * 24 * 60 * 60 * 1000;
export const QUOTA_BYTES = 4 * 1024 * 1024;
export const DEBOUNCE_MS = 400;

export const draftImageSchema = z.object({
  base64: z.string(),
  name: z.string(),
  mime: z.string(),
});

export const draftSchema = z.object({
  text: z.string(),
  image: draftImageSchema.nullable(),
  savedAt: z.number(),
});

export type TextImageDraft = z.infer<typeof draftSchema>;
export type TextImageDraftImage = z.infer<typeof draftImageSchema>;

export function shouldExpire(savedAt: number, now: number, ttlMs = TTL_MS): boolean {
  return savedAt + ttlMs < now;
}

// Returns the JSON string length in UTF-16 code units, not bytes.
// localStorage quotas in browsers are measured in UTF-16 units (2 bytes each),
// so comparing against QUOTA_BYTES (expressed in those same units) is correct.
export function estimateSerializedLength(draft: TextImageDraft): number {
  try {
    return JSON.stringify(draft).length;
  } catch {
    return Number.POSITIVE_INFINITY;
  }
}

export function applyQuotaFallback(
  draft: TextImageDraft,
  limit = QUOTA_BYTES,
): { draft: TextImageDraft; dropped: boolean } {
  if (estimateSerializedLength(draft) <= limit) {
    return { draft, dropped: false };
  }
  return {
    draft: { text: draft.text, image: null, savedAt: draft.savedAt },
    dropped: true,
  };
}

// True when the draft has no image yet still exceeds the quota —
// i.e. text alone is oversize. Writing it would trip the persistence
// layer's QuotaExceededError path; callers should clear the slot instead.
export function isTextOnlyOversize(draft: TextImageDraft, limit = QUOTA_BYTES): boolean {
  return draft.image === null && estimateSerializedLength(draft) > limit;
}

export function hasContent(text: string, hasImage: boolean): boolean {
  return Boolean(text.trim()) || hasImage;
}

export function emptyDraft(): TextImageDraft {
  return { text: '', image: null, savedAt: 0 };
}

export function encodeBytesToBase64(bytes: Uint8Array): string {
  let binary = '';
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    const slice = bytes.subarray(i, i + chunk);
    binary += String.fromCharCode(...slice);
  }
  return btoa(binary);
}

export function decodeBase64ToBytes(base64: string): Uint8Array {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}
