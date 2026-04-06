import { useCallback, useEffect, useRef, useState } from 'react';
import log from '#renderer/logger';

const DEBOUNCE_MS = 400;

/**
 * Persist a text draft to localStorage with debounced writes.
 * Returns [value, setValue, clearDraft].
 *
 * - Loads from localStorage on mount.
 * - Debounces writes (400ms) on every setValue call.
 * - clearDraft() removes the key and resets value to ''.
 * - Flushes pending writes on unmount.
 */
export function useDraft(key: string): [string, (v: string) => void, () => void] {
  const [value, setValueState] = useState(() => {
    try {
      return localStorage.getItem(key) ?? '';
    } catch (err) {
      log.warn(`[useDraft] initial load failed for key="${key}":`, err);
      return '';
    }
  });

  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const latestRef = useRef(value);
  const keyRef = useRef(key);

  // When the key changes (e.g. switching tasks/modes), flush pending write
  // to the OLD key before loading the new one. keyRef is updated inside
  // the effect (after flush) so the timer always targets the correct key.
  useEffect(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
      const prevKey = keyRef.current;
      try {
        if (latestRef.current) localStorage.setItem(prevKey, latestRef.current);
        else localStorage.removeItem(prevKey);
      } catch (err) {
        log.warn(`[useDraft] flush on key change failed for key="${prevKey}":`, err);
      }
    }
    keyRef.current = key;
    try {
      const stored = localStorage.getItem(key) ?? '';
      setValueState(stored);
      latestRef.current = stored;
    } catch (err) {
      log.warn(`[useDraft] load failed for key="${key}":`, err);
      setValueState('');
      latestRef.current = '';
    }
  }, [key]);

  const scheduleSave = useCallback((v: string) => {
    latestRef.current = v;
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => {
      timerRef.current = null;
      try {
        if (latestRef.current) localStorage.setItem(keyRef.current, latestRef.current);
        else localStorage.removeItem(keyRef.current);
      } catch (err) {
        log.warn(`[useDraft] save failed for key="${keyRef.current}":`, err);
      }
    }, DEBOUNCE_MS);
  }, []);

  const setValue = useCallback(
    (v: string) => {
      setValueState(v);
      scheduleSave(v);
    },
    [scheduleSave],
  );

  const clearDraft = useCallback(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    setValueState('');
    latestRef.current = '';
    try {
      localStorage.removeItem(keyRef.current);
    } catch (err) {
      log.warn(`[useDraft] clear failed for key="${keyRef.current}":`, err);
    }
  }, []);

  // Flush pending write on unmount.
  useEffect(() => {
    return () => {
      if (timerRef.current) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
        try {
          if (latestRef.current) localStorage.setItem(keyRef.current, latestRef.current);
          else localStorage.removeItem(keyRef.current);
        } catch (err) {
          log.warn(`[useDraft] unmount flush failed for key="${keyRef.current}":`, err);
        }
      }
    };
  }, []);

  return [value, setValue, clearDraft];
}

/**
 * Like useDraft but stores a Record<number, string> as JSON.
 * Used for clarification answers where multiple fields share one key.
 */
export function useDraftRecord(
  key: string,
): [Record<number, string>, (v: Record<number, string>) => void, () => void] {
  const [raw, setRaw, clearRaw] = useDraft(key);

  // Parse during render but never call side effects (clearRaw) here —
  // invalid data returns {} and will be overwritten on next valid write.
  const parsed: Record<number, string> = raw
    ? (() => {
        try {
          const val = JSON.parse(raw);
          if (typeof val !== 'object' || val === null || Array.isArray(val)) {
            log.warn(`[useDraftRecord] invalid shape for key="${key}"`);
            return {};
          }
          return val as Record<number, string>;
        } catch (err) {
          log.warn(`[useDraftRecord] parse failed for key="${key}":`, err);
          return {};
        }
      })()
    : {};

  const setValue = useCallback(
    (v: Record<number, string>) => {
      const hasContent = Object.values(v).some((s) => s.trim());
      setRaw(hasContent ? JSON.stringify(v) : '');
    },
    [setRaw],
  );

  return [parsed, setValue, clearRaw];
}
