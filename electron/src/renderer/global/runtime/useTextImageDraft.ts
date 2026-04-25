import { useCallback, useRef, useState } from 'react';
import log from '#renderer/global/service/logger';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import {
  DEBOUNCE_MS,
  emptyDraft,
  type TextImageDraft,
  type TextImageDraftImage,
} from '#renderer/global/runtime/useTextImageDraft.helpers';
import {
  hydrateImage,
  loadInitial,
  resetDraftSlot,
  runGuardedEncode,
  writeDraftToSlot,
} from '#renderer/global/runtime/useTextImageDraft.slots';

export interface TextImageDraftState {
  text: string;
  setText: (v: string) => void;
  image: File | null;
  preview: string | null;
  setImageFile: (file: File) => void;
  removeImage: () => void;
  clearDraft: () => void;
  /**
   * Clear the persisted v2 slot and cancel pending writes, but leave the
   * visible React state alone. Used by submit flows where the parent unmounts
   * the composer a moment later — avoids flashing the form blank before close.
   */
  clearDraftStorage: () => void;
}

export interface TextImageDraftOptions {
  /**
   * Legacy `mando:draft:<suffix>` text-only key to seed from on first v2 read.
   * Cleared after seeding so the two draft systems never run in parallel.
   */
  legacyTextSuffix?: string;
  /** Default text to show on first mount when no draft is present. */
  initialText?: string;
}

export function useTextImageDraft(
  key: string,
  options: TextImageDraftOptions = {},
): TextImageDraftState {
  const { legacyTextSuffix, initialText } = options;
  const [draft, setDraft] = useState<TextImageDraft>(() => {
    try {
      return loadInitial(key, legacyTextSuffix, initialText, Date.now());
    } catch (err) {
      log.warn(`[useTextImageDraft] initial load failed for key="${key}":`, err);
      return emptyDraft();
    }
  });
  const [preview, setPreview] = useState<string | null>(null);
  const [imageFile, setImageFileState] = useState<File | null>(null);
  const previewRef = useRef<string | null>(null);
  const latestRef = useRef<TextImageDraft>(draft);
  const keyRef = useRef(key);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const quotaWarnedRef = useRef(false);
  // Invalidates late-resolving base64 encodes whose state has been superseded.
  const imageGenRef = useRef(0);

  // Render-time key-change handler: matches the pattern in useImageAttachment.
  if (keyRef.current !== key) {
    const prevKey = keyRef.current;
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
      writeDraftToSlot(prevKey, latestRef.current, quotaWarnedRef);
    }
    if (previewRef.current) URL.revokeObjectURL(previewRef.current);
    previewRef.current = null;
    keyRef.current = key;
    quotaWarnedRef.current = false;
    imageGenRef.current += 1;
    const next = loadInitial(key, legacyTextSuffix, initialText, Date.now());
    latestRef.current = next;
    setDraft(next);
    const hydrated = next.image ? hydrateImage(next.image, key) : null;
    if (hydrated) {
      setImageFileState(hydrated.file);
      setPreview(hydrated.preview);
      previewRef.current = hydrated.preview;
    } else {
      setImageFileState(null);
      setPreview(null);
    }
  }

  useMountEffect(() => {
    const img = latestRef.current.image;
    if (img) {
      const hydrated = hydrateImage(img, keyRef.current);
      if (hydrated) {
        setImageFileState(hydrated.file);
        setPreview(hydrated.preview);
        previewRef.current = hydrated.preview;
      }
    }
    return () => {
      imageGenRef.current += 1;
      if (timerRef.current) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
        writeDraftToSlot(keyRef.current, latestRef.current, quotaWarnedRef);
      }
      if (previewRef.current) URL.revokeObjectURL(previewRef.current);
      previewRef.current = null;
    };
  });

  const scheduleSave = useCallback((next: TextImageDraft) => {
    latestRef.current = next;
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => {
      timerRef.current = null;
      writeDraftToSlot(keyRef.current, latestRef.current, quotaWarnedRef);
    }, DEBOUNCE_MS);
  }, []);

  const setText = useCallback(
    (v: string) => {
      const next: TextImageDraft = { ...latestRef.current, text: v, savedAt: Date.now() };
      setDraft(next);
      scheduleSave(next);
    },
    [scheduleSave],
  );

  const finalizeImage = useCallback(
    (file: File, base64: string) => {
      const img: TextImageDraftImage = {
        base64,
        name: file.name,
        mime: file.type || 'application/octet-stream',
      };
      quotaWarnedRef.current = false;
      const next: TextImageDraft = { ...latestRef.current, image: img, savedAt: Date.now() };
      setDraft(next);
      scheduleSave(next);
    },
    [scheduleSave],
  );

  const setImageFile = useCallback(
    (file: File) => {
      if (previewRef.current) URL.revokeObjectURL(previewRef.current);
      const url = URL.createObjectURL(file);
      setImageFileState(file);
      setPreview(url);
      previewRef.current = url;
      runGuardedEncode(file, imageGenRef, keyRef, finalizeImage);
    },
    [finalizeImage],
  );

  const removeImage = useCallback(() => {
    imageGenRef.current += 1;
    if (previewRef.current) URL.revokeObjectURL(previewRef.current);
    previewRef.current = null;
    setPreview(null);
    setImageFileState(null);
    quotaWarnedRef.current = false;
    const next: TextImageDraft = { ...latestRef.current, image: null, savedAt: Date.now() };
    setDraft(next);
    scheduleSave(next);
  }, [scheduleSave]);

  const clearDraft = useCallback(() => {
    resetDraftSlot(keyRef, imageGenRef, timerRef, latestRef, quotaWarnedRef);
    if (previewRef.current) URL.revokeObjectURL(previewRef.current);
    previewRef.current = null;
    setPreview(null);
    setImageFileState(null);
    setDraft(emptyDraft());
  }, []);

  // Storage-only clear: unmount cleanup sees hasContent=false via latestRef
  // and skips re-persisting the React state that the parent is about to unmount.
  const clearDraftStorage = useCallback(() => {
    resetDraftSlot(keyRef, imageGenRef, timerRef, latestRef, quotaWarnedRef);
  }, []);

  return {
    text: draft.text,
    setText,
    image: imageFile,
    preview,
    setImageFile,
    removeImage,
    clearDraft,
    clearDraftStorage,
  };
}
