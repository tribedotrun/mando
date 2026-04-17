import React, { useRef, useState } from 'react';
import { copyToClipboard } from '#renderer/global/service/utils';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import {
  buildInspectResult,
  findOwnerComponent,
  getFiber,
  installGlobals,
  removeGlobals,
} from '#renderer/global/service/devInspector';

const TOAST_DISPLAY_MS = 2000;

export function DevInspector({
  active,
  onHover,
}: {
  active: boolean;
  onHover: (name: string | null) => void;
}): React.ReactElement | null {
  const highlightRef = useRef<HTMLDivElement>(null);
  const labelRef = useRef<HTMLDivElement>(null);
  const hoveredRef = useRef<HTMLElement | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [labelText, setLabelText] = useState<string | null>(null);

  const activeRef = useRef(active);
  activeRef.current = active;
  const onHoverRef = useRef(onHover);
  onHoverRef.current = onHover;

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
      setTimeout(() => setToast(null), TOAST_DISPLAY_MS);
    }
  };

  const doCopyRef = useRef(doCopy);
  doCopyRef.current = doCopy;

  // Attach globals on mount, clean up on unmount
  useMountEffect(() => {
    installGlobals(doCopyRef);
    return removeGlobals;
  });

  if (!active) return null;

  return (
    <>
      <div
        ref={highlightRef}
        style={{
          position: 'fixed',
          display: 'none',
          pointerEvents: 'none',
          border: '2px solid var(--ring)',
          background: 'var(--accent)',
          borderRadius: 4,
          zIndex: 99998,
          transition: 'all 50ms ease-out',
        }}
      />
      <div
        ref={labelRef}
        style={{
          position: 'fixed',
          display: 'none',
          pointerEvents: 'none',
          background: 'var(--foreground)',
          color: 'var(--background)',
          fontSize: 11,
          fontFamily: 'monospace',
          padding: '2px 6px',
          borderRadius: 4,
          zIndex: 99999,
          whiteSpace: 'nowrap',
        }}
      >
        {labelText}
      </div>
      {toast && (
        <div
          className="flex items-center gap-2"
          style={{
            position: 'fixed',
            bottom: 32,
            right: 16,
            background: 'var(--muted)',
            border: '1px solid var(--border)',
            borderRadius: 6,
            padding: '4px 8px',
            zIndex: 100000,
            fontFamily: 'monospace',
            fontSize: 11,
            color: 'var(--text-3)',
            pointerEvents: 'none',
          }}
        >
          <span className="text-[11px] text-success">✓ copied</span>
          <span className="max-w-[300px] truncate">{toast}</span>
        </div>
      )}
    </>
  );
}
