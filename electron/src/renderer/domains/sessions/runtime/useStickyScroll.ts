import { useEffect, useRef, useState } from 'react';

const BOTTOM_EPSILON_PX = 24;

/**
 * Minimal sticky-scroll controller. Keeps the scroll pinned to the bottom
 * while the user has not scrolled up; any upward scroll detaches until the
 * user returns to within a few pixels of the bottom.
 *
 * Exposes `isAtBottom` as React state so the caller can toggle UI (e.g., a
 * scroll-to-latest button) without waiting for an unrelated re-render. The
 * scroll listener only calls `setIsAtBottom` when the boolean actually
 * flips, so keeping state instead of a ref costs nothing on the hot path.
 */
export interface StickyScrollHandle {
  scrollRef: React.RefObject<HTMLDivElement | null>;
  isAtBottom: boolean;
  scrollToBottom: (smooth?: boolean) => void;
}

export function useStickyScroll(dep: unknown): StickyScrollHandle {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const [isAtBottom, setIsAtBottom] = useState(true);
  const isAtBottomRef = useRef(true);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return undefined;
    const handleScroll = () => {
      const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
      const nextAtBottom = distanceFromBottom <= BOTTOM_EPSILON_PX;
      if (nextAtBottom === isAtBottomRef.current) return;
      isAtBottomRef.current = nextAtBottom;
      setIsAtBottom(nextAtBottom);
    };
    el.addEventListener('scroll', handleScroll, { passive: true });
    return () => el.removeEventListener('scroll', handleScroll);
  }, []);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el || !isAtBottomRef.current) return;
    el.scrollTop = el.scrollHeight;
  }, [dep]);

  const scrollToBottom = (smooth = true) => {
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTo({ top: el.scrollHeight, behavior: smooth ? 'smooth' : 'auto' });
    isAtBottomRef.current = true;
    setIsAtBottom(true);
  };

  return { scrollRef, isAtBottom, scrollToBottom };
}
