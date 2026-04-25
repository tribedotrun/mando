import {
  defineJsonKeyspace,
  defineKeyspace,
  type PersistedJsonSlot,
  type PersistedSlot,
} from '#renderer/global/providers/persistence';
import log from '#renderer/global/service/logger';
import {
  applyQuotaFallback,
  decodeBase64ToBytes,
  draftSchema,
  emptyDraft,
  encodeBytesToBase64,
  hasContent,
  isTextOnlyOversize,
  shouldExpire,
  type TextImageDraft,
  type TextImageDraftImage,
} from '#renderer/global/runtime/useTextImageDraft.helpers';

const V2_PREFIX = 'mando:draft:v2:';
const V1_PREFIX = 'mando:draft:';
const OWNER = 'global/runtime/useTextImageDraft';

const draftStore = defineJsonKeyspace(V2_PREFIX, draftSchema, OWNER);
const legacyTextStore = defineKeyspace(V1_PREFIX, OWNER);

export function draftSlotFor(suffix: string): PersistedJsonSlot<TextImageDraft> {
  return draftStore.for(suffix);
}

function legacyTextSlotFor(suffix: string): PersistedSlot {
  return legacyTextStore.for(suffix);
}

export function reconstructFile(image: TextImageDraftImage): File {
  const bytes = decodeBase64ToBytes(image.base64);
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  return new File([copy.buffer], image.name, { type: image.mime });
}

// invariant: native Promise is required here — callers wrap in IIFE + try/catch.
export async function readFileAsBase64(file: File) {
  const buffer = await file.arrayBuffer();
  return encodeBytesToBase64(new Uint8Array(buffer));
}

// Encode `file` to base64, then call `finalize` only if no later image transition
// (pick/remove/clear/key-change/unmount) has invalidated this attempt.
export function runGuardedEncode(
  file: File,
  imageGenRef: React.MutableRefObject<number>,
  keyRef: React.MutableRefObject<string>,
  finalize: (file: File, base64: string) => void,
): void {
  imageGenRef.current += 1;
  const gen = imageGenRef.current;
  const keyAtCall = keyRef.current;
  void (async () => {
    try {
      const base64 = await readFileAsBase64(file);
      if (imageGenRef.current !== gen || keyRef.current !== keyAtCall) return;
      finalize(file, base64);
    } catch (err) {
      log.warn(`[useTextImageDraft] base64 encode failed for key="${keyRef.current}":`, err);
    }
  })();
}

export function loadInitial(
  suffix: string,
  legacyTextSuffix: string | undefined,
  initialText: string | undefined,
  now: number,
): TextImageDraft {
  const slot = draftSlotFor(suffix);
  const stored = slot.read();
  if (stored) {
    if (shouldExpire(stored.savedAt, now)) {
      slot.clear();
    } else {
      return stored;
    }
  }
  if (legacyTextSuffix !== undefined) {
    const legacySlot = legacyTextSlotFor(legacyTextSuffix);
    const legacyText = legacySlot.read();
    if (legacyText) {
      // Persist to v2 before wiping v1 so a user who opens the composer
      // without editing does not silently lose the legacy text on next mount.
      const migrated: TextImageDraft = { text: legacyText, image: null, savedAt: now };
      draftSlotFor(suffix).write(migrated);
      legacySlot.clear();
      return migrated;
    }
    legacySlot.clear();
  }
  if (initialText) return { text: initialText, image: null, savedAt: now };
  return emptyDraft();
}

export function writeDraftToSlot(
  slotKey: string,
  next: TextImageDraft,
  quotaWarnedRef: React.MutableRefObject<boolean>,
): void {
  const slot = draftSlotFor(slotKey);
  if (!hasContent(next.text, next.image !== null)) {
    slot.clear();
    return;
  }
  const { draft: toWrite, dropped } = applyQuotaFallback(next);
  // Text alone exceeds quota even after dropping the image. Writing would fail
  // inside the persistence layer with a QuotaExceededError; clear the slot
  // explicitly so we do not leave a stale half-written draft.
  if (isTextOnlyOversize(toWrite)) {
    if (!quotaWarnedRef.current) {
      quotaWarnedRef.current = true;
      log.warn(
        `[useTextImageDraft] text exceeds quota for key="${slotKey}"; clearing draft (text alone too large for localStorage).`,
      );
    }
    slot.clear();
    return;
  }
  if (dropped) {
    if (!quotaWarnedRef.current) {
      quotaWarnedRef.current = true;
      log.warn(
        `[useTextImageDraft] draft exceeds quota for key="${slotKey}"; dropping image, keeping text.`,
      );
    }
  } else if (toWrite.image !== null) {
    quotaWarnedRef.current = false;
  }
  slot.write(toWrite);
}

// Invalidate in-flight encode/debounce guards and drop the persisted slot.
// Leaves React state alone so callers can decide whether to also reset the UI.
export function resetDraftSlot(
  keyRef: React.MutableRefObject<string>,
  imageGenRef: React.MutableRefObject<number>,
  timerRef: React.MutableRefObject<ReturnType<typeof setTimeout> | null>,
  latestRef: React.MutableRefObject<TextImageDraft>,
  quotaWarnedRef: React.MutableRefObject<boolean>,
): void {
  imageGenRef.current += 1;
  if (timerRef.current) {
    clearTimeout(timerRef.current);
    timerRef.current = null;
  }
  quotaWarnedRef.current = false;
  latestRef.current = emptyDraft();
  draftSlotFor(keyRef.current).clear();
}

export interface HydratedImage {
  file: File;
  preview: string;
}

export function hydrateImage(image: TextImageDraftImage, key: string): HydratedImage | null {
  try {
    const file = reconstructFile(image);
    const preview = URL.createObjectURL(file);
    return { file, preview };
  } catch (err) {
    log.warn(`[useTextImageDraft] image hydrate failed for key="${key}":`, err);
    return null;
  }
}
