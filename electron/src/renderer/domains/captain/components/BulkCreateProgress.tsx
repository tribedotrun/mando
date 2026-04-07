import React from 'react';
import { X } from 'lucide-react';
import {
  useBulkCreateStore,
  type BulkCreatePhase,
} from '#renderer/domains/captain/stores/bulkCreateStore';
import { Spinner } from '#renderer/global/components/Spinner';

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
        {isActive && <Spinner />}

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
            <X size={14} />
          </button>
        )}
      </div>
    </>
  );
}
