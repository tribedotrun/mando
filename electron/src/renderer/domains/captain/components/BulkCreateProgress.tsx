import React from 'react';
import {
  useBulkCreateStore,
  type BulkCreatePhase,
} from '#renderer/domains/captain/stores/bulkCreateStore';

function progressText(phase: BulkCreatePhase): string {
  switch (phase.step) {
    case 'parsing':
      return 'Parsing tasks…';
    case 'creating':
      return `Adding ${phase.done}/${phase.total}…`;
    case 'done':
      return `Added ${phase.count} task${phase.count === 1 ? '' : 's'}`;
    case 'error':
      return phase.message;
    case 'idle':
      return '';
  }
}

export function BulkCreateProgress(): React.ReactElement | null {
  const phase = useBulkCreateStore((s) => s.phase);
  const dismiss = useBulkCreateStore((s) => s.dismiss);

  if (phase.step === 'idle') return null;

  const isActive = phase.step === 'parsing' || phase.step === 'creating';
  const isError = phase.step === 'error';

  return (
    <>
      <style>{`
        @keyframes bulk-in {
          from { opacity: 0; transform: translateY(8px); }
          to   { opacity: 1; transform: translateY(0); }
        }
      `}</style>
      <div
        className="fixed z-[350] flex items-center gap-2 rounded-lg px-4 py-2"
        style={{
          bottom: 16,
          left: 16,
          background: 'var(--color-surface-2)',
          border: '1px solid var(--color-border-subtle)',
          boxShadow: '0 8px 24px #00000066',
          animation: 'bulk-in 200ms ease-out',
        }}
      >
        {isActive && (
          <span
            className="animate-spin"
            style={{
              width: 14,
              height: 14,
              borderRadius: 8,
              border: '2px solid var(--color-accent)',
              borderTopColor: 'transparent',
              flexShrink: 0,
            }}
          />
        )}

        <span
          className="text-[13px] font-medium"
          style={{ color: isError ? 'var(--color-error)' : 'var(--color-text-1)' }}
        >
          {progressText(phase)}
        </span>

        {!isActive && (
          <button
            onClick={dismiss}
            className="flex items-center justify-center"
            style={{
              color: 'var(--color-text-3)',
              cursor: 'pointer',
              background: 'none',
              border: 'none',
              padding: 0,
              marginLeft: 2,
            }}
            aria-label="Dismiss"
          >
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
              <path
                d="M4 4L10 10M10 4L4 10"
                stroke="currentColor"
                strokeWidth="1.5"
                strokeLinecap="round"
              />
            </svg>
          </button>
        )}
      </div>
    </>
  );
}
