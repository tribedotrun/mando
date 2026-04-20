import { useRef, useState } from 'react';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import { useDevInfo } from '#renderer/global/runtime/useAppInfo';

export function useDevInfoBar() {
  const info = useDevInfo();
  const [inspecting, setInspecting] = useState(false);
  const [hoveredName, setHoveredName] = useState<string | null>(null);
  const inspectingRef = useRef(false);
  inspectingRef.current = inspecting;

  // Shift+A: toggle inspect on, or copy when already on (dev/sandbox only)
  const infoRef = useRef(info);
  infoRef.current = info;
  useMountEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!infoRef.current) return;
      const t = e.target as HTMLElement;
      if (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t.isContentEditable)
        return;
      if (e.key === 'A' && e.shiftKey && !e.metaKey && !e.ctrlKey) {
        e.preventDefault();
        if (inspectingRef.current) {
          const copy = window.__devInspectorCopy;
          if (copy) copy();
        } else {
          setHoveredName(null);
          setInspecting(true);
        }
      } else if (e.key === 'Escape' && inspectingRef.current) {
        setHoveredName(null);
        setInspecting(false);
      }
    };
    document.addEventListener('keydown', onKey, true);
    return () => document.removeEventListener('keydown', onKey, true);
  });

  return { info, inspecting, setInspecting, hoveredName, setHoveredName };
}
