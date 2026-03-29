import React, { useState } from 'react';
import { processScout } from '#renderer/api';
import { useToastStore } from '#renderer/stores/toastStore';

interface Props {
  onDone: () => void;
}

export function ScoutActions({ onDone }: Props): React.ReactElement {
  const [processPending, setProcessPending] = useState(false);

  const handleProcessAll = async () => {
    setProcessPending(true);
    try {
      await processScout();
      onDone();
    } catch (err) {
      useToastStore
        .getState()
        .add('error', `Process failed: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setProcessPending(false);
    }
  };

  return (
    <div data-testid="scout-actions" className="flex shrink-0 items-center gap-2">
      <button
        onClick={handleProcessAll}
        disabled={processPending}
        className="shrink-0 text-[13px] font-semibold disabled:opacity-50"
        style={{
          background: 'var(--color-accent)',
          color: 'var(--color-bg)',
          padding: '6px 16px',
          borderRadius: 'var(--radius-button)',
          border: 'none',
          cursor: 'pointer',
        }}
      >
        {processPending ? 'Processing...' : 'Process all pending'}
      </button>
    </div>
  );
}
