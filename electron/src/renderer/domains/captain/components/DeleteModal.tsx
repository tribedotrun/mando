import React, { useState } from 'react';
import type { TaskItem } from '#renderer/types';
import { Dialog, DialogContent, DialogTitle } from '#renderer/global/components/Dialog';

interface Props {
  items: TaskItem[];
  deleting: boolean;
  error: string | null;
  onConfirm: (opts: { close_pr: boolean }) => void;
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

  const [closePr, setClosePr] = useState(false);

  return (
    <Dialog open={true} onOpenChange={() => onCancel()}>
      <DialogContent data-testid="delete-modal" className="max-h-[80vh] overflow-y-auto">
        <DialogTitle>Delete Items</DialogTitle>

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

        {hasPr && (
          <div className="mb-3 flex flex-col gap-2 rounded-md px-3 py-2 bg-surface-3">
            <label className="flex cursor-pointer items-center gap-2 text-[13px]">
              <input
                type="checkbox"
                checked={closePr}
                onChange={(e) => setClosePr(e.target.checked)}
                style={{ accentColor: 'var(--color-accent)' }}
              />
              <span className="text-text-2">Close associated PRs</span>
            </label>
          </div>
        )}

        {error && (
          <p className="mb-2 rounded-md px-3 py-2 text-[13px] bg-error-bg text-error">{error}</p>
        )}

        <div className="flex justify-end gap-2">
          <button
            onClick={onCancel}
            className="rounded-md px-3 py-2 text-[13px]"
            style={{
              background: 'transparent',
              color: 'var(--color-text-2)',
              border: '1px solid var(--color-border)',
            }}
          >
            Cancel
          </button>
          <button
            onClick={() => onConfirm({ close_pr: closePr })}
            disabled={!canDelete || deleting}
            className="rounded-md px-4 py-2 text-[13px] font-semibold disabled:opacity-50"
            style={{
              background: 'transparent',
              border: '1px solid var(--color-border-destructive)',
              color: 'var(--color-error)',
            }}
          >
            {deleting ? 'Deleting...' : `Delete ${safe.length} item${safe.length !== 1 ? 's' : ''}`}
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
