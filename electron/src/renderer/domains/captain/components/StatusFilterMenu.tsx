import React from 'react';

const STATUS_OPTIONS = ['all', 'running', 'stopped', 'failed'] as const;

export function StatusFilterMenu({
  value,
  onChange,
  onClose,
}: {
  value: string;
  onChange: (v: string) => void;
  onClose: () => void;
}): React.ReactElement {
  return (
    <>
      {/* Invisible backdrop to catch outside clicks */}
      <div style={{ position: 'fixed', inset: 0, zIndex: 49 }} onMouseDown={onClose} />
      <div
        style={{
          position: 'absolute',
          top: '100%',
          right: 0,
          marginTop: 4,
          background: 'var(--color-surface-2)',
          border: '1px solid var(--color-border)',
          borderRadius: 8,
          padding: '4px 0',
          minWidth: 140,
          zIndex: 50,
          boxShadow: '0 8px 24px rgba(0,0,0,0.4)',
        }}
      >
        <div style={{ padding: '4px 12px 6px', fontSize: 11, color: 'var(--color-text-4)' }}>
          Status
        </div>
        {STATUS_OPTIONS.map((opt) => {
          const active = value === opt;
          return (
            <button
              key={opt}
              onClick={() => onChange(opt)}
              className="flex w-full items-center text-[12px]"
              style={{
                padding: '6px 12px',
                border: 'none',
                cursor: 'pointer',
                background: 'transparent',
                color: active ? 'var(--color-text-1)' : 'var(--color-text-3)',
                gap: 8,
              }}
            >
              <span className="flex-1 text-left" style={{ textTransform: 'capitalize' }}>
                {opt}
              </span>
              {active && (
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
                  <path
                    d="M3.5 8.5L6.5 11.5L12.5 4.5"
                    stroke="currentColor"
                    strokeWidth="1.5"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  />
                </svg>
              )}
            </button>
          );
        })}
      </div>
    </>
  );
}
