import { useCallback, useRef } from 'react';

/** Returns a debounced version of the callback. The latest call wins. */
export function useDebouncedCallback<T extends (...args: never[]) => void>(
  callback: T,
  delayMs: number,
): T {
  const timerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const cbRef = useRef(callback);
  cbRef.current = callback;

  return useCallback(
    ((...args: Parameters<T>) => {
      clearTimeout(timerRef.current);
      timerRef.current = setTimeout(() => cbRef.current(...args), delayMs);
    }) as T,
    [delayMs],
  );
}
