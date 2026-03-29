import React, { useMemo } from 'react';
import type { TaskItem } from '#renderer/types';

interface Props {
  items: TaskItem[];
  deleting: boolean;
  error: string | null;
  onConfirm: () => void;
  onCancel: () => void;
}

export function DeleteModal({
  items,
  deleting,
  error,
  onConfirm,
  onCancel,
}: Props): React.ReactElement {
  const inProgress = useMemo(() => items.filter((b) => b.status === 'in-progress'), [items]);
  const safe = useMemo(() => items.filter((b) => b.status !== 'in-progress'), [items]);
  const canDelete = safe.length > 0;

  return (
    <div
      data-testid="delete-modal"
      className="fixed inset-0 z-[200] flex items-center justify-center bg-black/60"
      onClick={(e) => e.target === e.currentTarget && onCancel()}
    >
      <div
        className="max-h-[80vh] w-[440px] max-w-[90vw] overflow-y-auto rounded-lg p-5"
        style={{ background: 'var(--color-surface-2)', border: '1px solid var(--color-border)' }}
      >
        <h3 className="text-subheading mb-3" style={{ color: 'var(--color-text-1)' }}>
          Delete Items
        </h3>

        <ul className="mb-3 max-h-[180px] overflow-y-auto">
          {safe.map((b) => (
            <li
              key={b.id}
              className="truncate py-1 text-[13px]"
              style={{
                color: 'var(--color-text-2)',
                borderBottom: '1px solid var(--color-border-subtle)',
              }}
            >
              {b.title}
            </li>
          ))}
          {inProgress.map((b) => (
            <li
              key={b.id}
              className="truncate py-1 text-[13px]"
              style={{
                color: 'var(--color-error)',
                borderBottom: '1px solid var(--color-border-subtle)',
              }}
            >
              {b.title} (in-progress -- skipped)
            </li>
          ))}
        </ul>

        {error && (
          <p
            className="mb-2 rounded-md px-3 py-2 text-[13px]"
            style={{ background: 'var(--color-error-bg)', color: 'var(--color-error)' }}
          >
            {error}
          </p>
        )}

        <div className="flex justify-end gap-2">
          <button
            onClick={onCancel}
            className="rounded-md px-3 py-1.5 text-[13px]"
            style={{ color: 'var(--color-text-2)', border: '1px solid var(--color-border)' }}
          >
            Cancel
          </button>
          <button
            onClick={() => onConfirm()}
            disabled={!canDelete || deleting}
            className="rounded-md px-4 py-1.5 text-[13px] font-semibold disabled:opacity-50"
            style={{ background: 'var(--color-error)', color: 'white' }}
          >
            {deleting ? 'Deleting...' : `Delete ${safe.length} item${safe.length !== 1 ? 's' : ''}`}
          </button>
        </div>
      </div>
    </div>
  );
}
