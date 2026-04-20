import { useCallback, useEffect, useRef, useState } from 'react';
import { z } from 'zod';
import {
  defineJsonKeyspace,
  defineKeyspace,
  type PersistedJsonSlot,
  type PersistedSlot,
} from '#renderer/global/providers/persistence';
import log from '#renderer/global/service/logger';

const DEBOUNCE_MS = 400;

const draftStore = defineKeyspace('', 'domains/captain/runtime/useDraft');
const draftRecordSchema = z
  .record(z.string().regex(/^\d+$/), z.string())
  .transform((value): Record<number, string> => value as Record<number, string>);
const draftRecordStore = defineJsonKeyspace(
  'json:',
  draftRecordSchema,
  'domains/captain/runtime/useDraft',
);

function slotFor(key: string): PersistedSlot {
  return draftStore.for(key);
}

function recordSlotFor(key: string): PersistedJsonSlot<Record<number, string>> {
  return draftRecordStore.for(key);
}

function readDraft(key: string): string {
  return slotFor(key).read() ?? '';
}

function writeDraft(key: string, value: string): void {
  if (value) slotFor(key).write(value);
  else slotFor(key).clear();
}

function readDraftRecord(key: string): Record<number, string> {
  return recordSlotFor(key).read() ?? {};
}

function writeDraftRecord(key: string, value: Record<number, string>): void {
  if (Object.values(value).some((entry) => entry.trim())) {
    recordSlotFor(key).write(value);
    return;
  }
  recordSlotFor(key).clear();
}

/**
 * Persist a text draft to browser storage with debounced writes.
 * Returns [value, setValue, clearDraft].
 *
 * - Loads on mount.
 * - Debounces writes (400ms) on every setValue call.
 * - clearDraft() removes the key and resets value to ''.
 * - Flushes pending writes on unmount.
 */
export function useDraft(key: string): [string, (v: string) => void, () => void] {
  const [value, setValueState] = useState(() => {
    try {
      return readDraft(key);
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
      writeDraft(prevKey, latestRef.current);
    }
    keyRef.current = key;
    const stored = readDraft(key);
    setValueState(stored);
    latestRef.current = stored;
  }, [key]);

  const scheduleSave = useCallback((v: string) => {
    latestRef.current = v;
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => {
      timerRef.current = null;
      writeDraft(keyRef.current, latestRef.current);
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
    slotFor(keyRef.current).clear();
  }, []);

  // Flush pending write on unmount.
  useEffect(() => {
    return () => {
      if (timerRef.current) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
        writeDraft(keyRef.current, latestRef.current);
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
  const [value, setValueState] = useState(() => {
    try {
      return readDraftRecord(key);
    } catch (err) {
      log.warn(`[useDraftRecord] initial load failed for key="${key}":`, err);
      return {};
    }
  });

  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const latestRef = useRef(value);
  const keyRef = useRef(key);

  useEffect(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
      const prevKey = keyRef.current;
      writeDraftRecord(prevKey, latestRef.current);
    }
    keyRef.current = key;
    const stored = readDraftRecord(key);
    setValueState(stored);
    latestRef.current = stored;
  }, [key]);

  const scheduleSave = useCallback((next: Record<number, string>) => {
    latestRef.current = next;
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => {
      timerRef.current = null;
      writeDraftRecord(keyRef.current, latestRef.current);
    }, DEBOUNCE_MS);
  }, []);

  const setValue = useCallback(
    (v: Record<number, string>) => {
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
    setValueState({});
    latestRef.current = {};
    recordSlotFor(keyRef.current).clear();
  }, []);

  useEffect(() => {
    return () => {
      if (timerRef.current) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
        writeDraftRecord(keyRef.current, latestRef.current);
      }
    };
  }, []);

  return [value, setValue, clearDraft];
}
