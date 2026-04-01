import React, { useState } from 'react';
import type { TaskItem } from '#renderer/types';
import { useFocusTrap } from '#renderer/hooks/useFocusTrap';

interface Props {
  items: TaskItem[];
  deleting: boolean;
  error: string | null;
  onConfirm: (opts: { close_pr: boolean; cancel_linear: boolean }) => void;
  onCancel: () => void;
}

export function DeleteModal({
  items,
  deleting,
  error,
  onConfirm,
  onCancel,
}: Props): React.ReactElement {
  const inProgress = items.filter((b) => b.status === 'in-progress');
  const safe = items.filter((b) => b.status !== 'in-progress');
  const canDelete = safe.length > 0;
  const hasPr = safe.some((b) => b.pr);
  const hasLinear = safe.some((b) => b.linear_id);

  const [closePr, setClosePr] = useState(false);
  const [cancelLinear, setCancelLinear] = useState(false);
  const { ref: dialogRef, handleKeyDown } = useFocusTrap(onCancel);

  return (
    <div
      data-testid="delete-modal"
      role="dialog"
      aria-modal="true"
      aria-label="Delete Items"
      className="fixed inset-0 z-[200] flex items-center justify-center bg-black/60"
      onClick={(e) => e.target === e.currentTarget && onCancel()}
      onKeyDown={handleKeyDown}
    >
      <div
        ref={dialogRef}
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

        {(hasPr || hasLinear) && (
          <div
            className="mb-3 flex flex-col gap-2 rounded-md px-3 py-2.5"
            style={{ background: 'var(--color-surface-3)' }}
          >
            {hasPr && (
              <label className="flex cursor-pointer items-center gap-2 text-[13px]">
                <input
                  type="checkbox"
                  checked={closePr}
                  onChange={(e) => setClosePr(e.target.checked)}
                  style={{ accentColor: 'var(--color-accent)' }}
                />
                <span style={{ color: 'var(--color-text-2)' }}>Close associated PRs</span>
              </label>
            )}
            {hasLinear && (
              <label className="flex cursor-pointer items-center gap-2 text-[13px]">
                <input
                  type="checkbox"
                  checked={cancelLinear}
                  onChange={(e) => setCancelLinear(e.target.checked)}
                  style={{ accentColor: 'var(--color-accent)' }}
                />
                <span style={{ color: 'var(--color-text-2)' }}>Cancel Linear issues</span>
              </label>
            )}
          </div>
        )}

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
            onClick={() => onConfirm({ close_pr: closePr, cancel_linear: cancelLinear })}
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
