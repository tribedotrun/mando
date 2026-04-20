import React from 'react';
import { useDevInspector } from '#renderer/global/runtime/useDevInspector';

export function DevInspector({
  active,
  onHover,
}: {
  active: boolean;
  onHover: (name: string | null) => void;
}): React.ReactElement | null {
  const { highlightRef, labelRef, labelText, toast } = useDevInspector(active, onHover);

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
