import React from 'react';
import type { TaskItem } from '#renderer/types';
import { prLabel } from '#renderer/utils';

interface Props {
  item: TaskItem;
  onConfirm: (itemId: number, pr: string, project: string) => void;
  onCancel: () => void;
  pending: boolean;
  result: { ok: boolean; message: string } | null;
}

export function MergeModal({
  item,
  onConfirm,
  onCancel,
  pending,
  result,
}: Props): React.ReactElement {
  const isDone = result?.ok;
  return (
    <div
      data-testid="merge-modal"
      className="fixed inset-0 z-[200] flex items-center justify-center bg-black/60"
      onClick={(e) => e.target === e.currentTarget && !pending && onCancel()}
    >
      <div
        className="w-[440px] max-w-[90vw] rounded-[8px] p-5"
        style={{ background: 'var(--color-surface-2)', border: '1px solid var(--color-border)' }}
      >
        <h3 className="text-subheading mb-3" style={{ color: 'var(--color-text-1)' }}>
          Merge {item.project?.split('/').pop()} PR {prLabel(item.pr ?? '')}
        </h3>

        <div className="mb-4">
          <p
            className="text-body truncate"
            style={{ color: 'var(--color-text-2)' }}
            title={item.title}
          >
            {item.title}
          </p>
          {item.branch && (
            <p
              className="text-code mt-1 truncate"
              style={{ color: 'var(--color-text-3)' }}
              title={item.branch}
            >
              {item.branch}
            </p>
          )}
        </div>

        {result ? (
          <div
            className="text-body mb-4 rounded-[6px] px-3 py-2"
            style={{
              background: result.ok ? 'var(--color-success-bg)' : 'var(--color-error-bg)',
              color: result.ok ? 'var(--color-success)' : 'var(--color-error)',
            }}
          >
            {result.message}
          </div>
        ) : (
          <p className="text-caption mb-4" style={{ color: 'var(--color-text-3)' }}>
            Squash and merge
          </p>
        )}

        <div className="flex justify-end gap-2">
          {!isDone && (
            <>
              <button
                onClick={onCancel}
                disabled={pending}
                className="rounded-[6px] px-5 py-2 text-[13px] font-medium disabled:opacity-50"
                style={{ color: 'var(--color-text-2)', border: '1px solid var(--color-border)' }}
              >
                Cancel
              </button>
              <button
                onClick={() => onConfirm(item.id, item.pr ?? '', item.project ?? '')}
                disabled={pending || !!result}
                className="rounded-[6px] px-5 py-2 text-[13px] font-semibold disabled:opacity-50"
                style={{ background: 'var(--color-accent)', color: 'var(--color-bg)' }}
              >
                {pending ? 'Merging...' : 'Merge'}
              </button>
            </>
          )}
          {result && !result.ok && (
            <button
              onClick={onCancel}
              className="rounded-[6px] px-5 py-2 text-[13px] font-medium"
              style={{ color: 'var(--color-text-2)' }}
            >
              Dismiss
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
