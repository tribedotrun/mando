import React, { useCallback, useRef } from 'react';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';

const FOCUSABLE =
  'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * Focus trap for modal dialogs.
 * Returns a ref to attach to the dialog container and a keydown handler for Tab/Escape.
 */
export function useFocusTrap(onClose: () => void) {
  const ref = useRef<HTMLDivElement>(null);

  useMountEffect(() => {
    ref.current?.querySelector<HTMLElement>('button, input, textarea')?.focus();
  });

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
        return;
      }
      if (e.key !== 'Tab') return;
      const el = ref.current;
      if (!el) return;
      const focusable = el.querySelectorAll<HTMLElement>(FOCUSABLE);
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    },
    [onClose],
  );

  return { ref, handleKeyDown };
}
