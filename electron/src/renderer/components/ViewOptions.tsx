import React, { useCallback, useRef, useState } from 'react';
import { useTaskStore } from '#renderer/stores/taskStore';
import { useMountEffect } from '#renderer/hooks/useMountEffect';
import { ToggleSwitch } from '#renderer/components/ToggleSwitch';

function ViewOptionsInner({
  pos,
  popoverRef,
  btnRef,
  onClose,
}: {
  pos: { top: number; left: number };
  popoverRef: React.RefObject<HTMLDivElement | null>;
  btnRef: React.RefObject<HTMLButtonElement | null>;
  onClose: () => void;
}): React.ReactElement {
  const { showArchived, setShowArchived } = useTaskStore();

  useMountEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    const onClick = (e: MouseEvent) => {
      const target = e.target as Node;
      if (btnRef.current?.contains(target) || popoverRef.current?.contains(target)) return;
      onClose();
    };
    document.addEventListener('keydown', onKey);
    document.addEventListener('mousedown', onClick);
    return () => {
      document.removeEventListener('keydown', onKey);
      document.removeEventListener('mousedown', onClick);
    };
  });

  return (
    <div
      ref={popoverRef}
      style={{
        position: 'fixed',
        top: pos.top,
        left: pos.left,
        zIndex: 200,
        background: 'var(--color-surface-2)',
        border: '1px solid var(--color-border)',
        borderRadius: 8,
        padding: '8px 0',
        minWidth: 200,
        boxShadow: '0 8px 24px rgba(0, 0, 0, 0.4)',
      }}
    >
      <div
        style={{
          padding: '4px 12px 8px',
          fontSize: 11,
          fontWeight: 500,
          color: 'var(--color-text-3)',
          letterSpacing: '0.06em',
          textTransform: 'uppercase',
        }}
      >
        View options
      </div>
      <div
        className="flex w-full items-center justify-between"
        style={{
          padding: '6px 12px',
          fontSize: 13,
          color: 'var(--color-text-1)',
        }}
      >
        <span>Show archived</span>
        <ToggleSwitch checked={showArchived} onChange={() => setShowArchived(!showArchived)} />
      </div>
    </div>
  );
}

export function ViewOptions(): React.ReactElement {
  const [open, setOpen] = useState(false);
  const btnRef = useRef<HTMLButtonElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);

  const [pos, setPos] = useState({ top: 0, left: 0 });

  const openPopover = useCallback(() => {
    if (btnRef.current) {
      const rect = btnRef.current.getBoundingClientRect();
      setPos({ top: rect.bottom + 4, left: rect.right - 200 });
    }
    setOpen(true);
  }, []);

  return (
    <>
      <button
        ref={btnRef}
        onClick={() => (open ? setOpen(false) : openPopover())}
        aria-label="View options"
        title="View options"
        className="flex items-center justify-center"
        style={{
          width: 28,
          height: 28,
          borderRadius: 6,
          border: 'none',
          background: open ? 'var(--color-surface-3)' : 'transparent',
          color: open ? 'var(--color-text-1)' : 'var(--color-text-3)',
          cursor: 'pointer',
          transition: 'background 120ms, color 120ms',
        }}
        onMouseEnter={(e) => {
          if (!open) e.currentTarget.style.background = 'var(--color-surface-3)';
        }}
        onMouseLeave={(e) => {
          if (!open) e.currentTarget.style.background = 'transparent';
        }}
      >
        <svg
          width="16"
          height="16"
          viewBox="0 0 16 16"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
        >
          <path d="M2 4h12M2 8h12M2 12h12" strokeLinecap="round" />
          <circle cx="5" cy="4" r="1.5" fill="currentColor" stroke="none" />
          <circle cx="11" cy="8" r="1.5" fill="currentColor" stroke="none" />
          <circle cx="7" cy="12" r="1.5" fill="currentColor" stroke="none" />
        </svg>
      </button>

      {open && (
        <ViewOptionsInner
          pos={pos}
          popoverRef={popoverRef}
          btnRef={btnRef}
          onClose={() => setOpen(false)}
        />
      )}
    </>
  );
}
