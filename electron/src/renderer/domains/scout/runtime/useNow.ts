import { useState } from 'react';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';

/** Returns Date.now() that updates at the given interval. */
export function useNow(intervalMs = 1000): number {
  const [now, setNow] = useState(Date.now);
  useMountEffect(() => {
    const id = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(id);
  });
  return now;
}
