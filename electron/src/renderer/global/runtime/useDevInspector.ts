import { useRef, useState } from 'react';
import { copyToClipboard } from '#renderer/global/runtime/useFeedback';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import {
  buildInspectResult,
  findOwnerComponent,
  getFiber,
  installGlobals,
  removeGlobals,
} from '#renderer/global/service/devInspector';

const TOAST_DISPLAY_MS = 2000;

export function useDevInspector(active: boolean, onHover: (name: string | null) => void) {
  const highlightRef = useRef<HTMLDivElement>(null);
  const labelRef = useRef<HTMLDivElement>(null);
  const hoveredRef = useRef<HTMLElement | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [labelText, setLabelText] = useState<string | null>(null);

  const activeRef = useRef(active);
  activeRef.current = active;
  const onHoverRef = useRef(onHover);
  onHoverRef.current = onHover;
  const toastTimerRef = useRef<number | null>(null);

  useMountEffect(() => {
    const onMouseMove = (e: MouseEvent) => {
      if (!activeRef.current || !highlightRef.current || !labelRef.current) return;
      const el = document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null;
      // When cursor enters toolbar, hide highlight but keep hoveredRef so Copy works
      if (!el || el.closest('[data-dev-toolbar]')) {
        highlightRef.current.style.display = 'none';
        labelRef.current.style.display = 'none';
        return;
      }

      hoveredRef.current = el;
      const rect = el.getBoundingClientRect();
      highlightRef.current.style.display = 'block';
      highlightRef.current.style.top = `${rect.top}px`;
      highlightRef.current.style.left = `${rect.left}px`;
      highlightRef.current.style.width = `${rect.width}px`;
      highlightRef.current.style.height = `${rect.height}px`;

      const fiber = getFiber(el);
      if (fiber) {
        const owner = findOwnerComponent(fiber);
        if (owner) {
          labelRef.current.style.display = 'block';
          labelRef.current.style.top = `${rect.top - 22 < 0 ? 0 : rect.top - 22}px`;
          labelRef.current.style.left = `${rect.left}px`;
          setLabelText(owner.name);
          onHoverRef.current(owner.name);
          return;
        }
      }
      labelRef.current.style.display = 'none';
      onHoverRef.current(null);
    };

    document.addEventListener('mousemove', onMouseMove, true);
    return () => {
      document.removeEventListener('mousemove', onMouseMove, true);
    };
  });

  // Called by DevInfoBar's Copy button
  const doCopy = async () => {
    const el = hoveredRef.current;
    if (!el) return;
    const info = buildInspectResult(el);
    if (!info) return;
    const ok = await copyToClipboard(JSON.stringify(info));
    if (ok) {
      setToast(`${info.component}${info.context.title ? ' — ' + info.context.title : ''}`);
      if (toastTimerRef.current !== null) window.clearTimeout(toastTimerRef.current);
      toastTimerRef.current = window.setTimeout(() => {
        setToast(null);
        toastTimerRef.current = null;
      }, TOAST_DISPLAY_MS);
    }
  };

  const doCopyRef = useRef(doCopy);
  doCopyRef.current = doCopy;

  useMountEffect(() => () => {
    if (toastTimerRef.current !== null) window.clearTimeout(toastTimerRef.current);
  });

  // Attach globals on mount, clean up on unmount
  useMountEffect(() => {
    installGlobals(doCopyRef);
    return removeGlobals;
  });

  return { highlightRef, labelRef, labelText, toast };
}
