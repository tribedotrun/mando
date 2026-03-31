import React from 'react';
import { CAPTAIN_TRIAGE_LABEL } from '#renderer/capabilityContract';

interface Props {
  onTriage: () => void;
  onStopAll: () => void;
}

const BTN_STYLE: React.CSSProperties = {
  background: 'var(--color-surface-1)',
  border: '1px solid var(--color-border-subtle)',
  color: 'var(--color-text-2)',
  borderRadius: 6,
  padding: '6px 10px',
  fontSize: 12,
  fontWeight: 600,
  cursor: 'pointer',
};

export function CaptainToolbar({ onTriage, onStopAll }: Props): React.ReactElement {
  return (
    <div className="mb-3 flex items-center gap-2">
      <button onClick={onTriage} style={BTN_STYLE}>
        {CAPTAIN_TRIAGE_LABEL}
      </button>
      <button
        onClick={onStopAll}
        style={{
          ...BTN_STYLE,
          color: 'var(--color-error)',
        }}
      >
        Stop workers
      </button>
    </div>
  );
}
