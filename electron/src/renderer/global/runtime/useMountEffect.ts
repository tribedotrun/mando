import { useEffect } from 'react';

/**
 * Blessed escape hatch: runs effect once on mount, cleanup on unmount.
 * This is the ONLY file that may call useEffect directly.
 */
export function useMountEffect(effect: () => void | (() => void)): void {
  useEffect(effect, []);
}
