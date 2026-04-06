import React from 'react';
import type { ItemStatus } from '#renderer/types';

const ALL_STATUSES: ItemStatus[] = [
  'new',
  'clarifying',
  'needs-clarification',
  'queued',
  'in-progress',
  'captain-reviewing',
  'captain-merging',
  'awaiting-review',
  'rework',
  'handed-off',
  'escalated',
  'errored',
  'merged',
  'completed-no-pr',
  'canceled',
];

interface Props {
  count: number;
  statuses?: string[];
  onDelete: () => void;
  onBulkStatus?: (status: string) => void;
  onCancel: () => void;
}

export function BulkBar({
  count,
  statuses,
  onDelete,
  onBulkStatus,
  onCancel,
}: Props): React.ReactElement | null {
  if (count === 0) return null;
  const statusList = statuses ?? ALL_STATUSES;

  return (
    <div
      data-testid="bulk-bar"
      className="fixed bottom-12 left-1/2 z-50 flex -translate-x-1/2 items-center gap-3 rounded-lg px-4 py-2 shadow-lg"
      style={{
        background: 'var(--color-surface-2)',
        border: '1px solid var(--color-accent)',
      }}
    >
      <span className="text-code tabular-nums" style={{ color: 'var(--color-accent)' }}>
        {count} selected
      </span>
      {onBulkStatus && (
        <>
          <div className="h-4 w-px" style={{ background: 'var(--color-border)' }} />
          <select
            onChange={(e) => {
              if (e.target.value) onBulkStatus(e.target.value);
              e.target.value = '';
            }}
            defaultValue=""
            aria-label="Set status for selected items"
            className="rounded-md px-2 py-1 text-[12px]"
            style={{
              background: 'var(--color-surface-3)',
              color: 'var(--color-text-2)',
              border: '1px solid var(--color-border)',
            }}
          >
            <option value="" disabled>
              set status...
            </option>
            {statusList.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
          </select>
        </>
      )}
      <button
        onClick={onDelete}
        className="rounded-md px-3 py-1 text-[12px] font-medium"
        style={{
          background: 'transparent',
          color: 'var(--color-error)',
          border: '1px solid var(--color-border-destructive)',
          borderRadius: 'var(--radius-button)',
        }}
      >
        Delete
      </button>
      <button
        onClick={onCancel}
        className="rounded-md px-3 py-1 text-[12px]"
        style={{ color: 'var(--color-text-3)', border: '1px solid var(--color-border)' }}
      >
        Clear
      </button>
    </div>
  );
}
