import { useCallback } from 'react';

/** Ref callback that scrolls the element into view when mounted. */
export function useScrollIntoViewRef(): (node: HTMLElement | null) => void {
  return useCallback((node: HTMLElement | null) => {
    node?.scrollIntoView({ block: 'nearest' });
  }, []);
}
