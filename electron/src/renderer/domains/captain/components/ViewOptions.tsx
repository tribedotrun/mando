import React, { useRef, useState } from 'react';
import { useTaskStore } from '#renderer/domains/captain/stores/taskStore';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { ToggleSwitch } from '#renderer/global/components/ToggleSwitch';

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
      <div className="text-label" style={{ padding: '4px 12px 8px', color: 'var(--color-text-3)' }}>
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

  const openPopover = () => {
    if (btnRef.current) {
      const rect = btnRef.current.getBoundingClientRect();
      setPos({ top: rect.bottom + 4, left: rect.right - 200 });
    }
    setOpen(true);
  };

  return (
    <>
      <button
        ref={btnRef}
        onClick={() => (open ? setOpen(false) : openPopover())}
        aria-label="View options"
        title="View options"
        className={`flex h-7 w-7 cursor-pointer items-center justify-center rounded-[var(--radius-button)] border-none transition-colors hover:bg-[var(--color-surface-3)] ${open ? 'bg-[var(--color-surface-3)] text-[var(--color-text-1)]' : 'bg-transparent text-[var(--color-text-3)]'}`}
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
