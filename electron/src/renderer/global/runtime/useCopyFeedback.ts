import { useCallback, useRef, useState } from 'react';

const DEFAULT_MS = 1200;

/** Manages the copied/not-copied toggle with auto-reset timer. */
export function useCopyFeedback(ms = DEFAULT_MS) {
  const [copied, setCopied] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  const markCopied = useCallback(() => {
    setCopied(true);
    clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => setCopied(false), ms);
  }, [ms]);

  return { copied, markCopied } as const;
}
