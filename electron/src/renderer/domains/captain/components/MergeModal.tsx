import React, { useCallback } from 'react';
import type { TaskItem } from '#renderer/types';
import { prLabel, shortRepo } from '#renderer/utils';
import { Dialog, DialogContent, DialogTitle } from '#renderer/global/components/Dialog';

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
  const guardedCancel = useCallback(() => {
    if (!pending) onCancel();
  }, [onCancel, pending]);

  return (
    <Dialog open={true} onOpenChange={guardedCancel}>
      <DialogContent data-testid="merge-modal" className="rounded-[8px]">
        <DialogTitle>
          Merge {shortRepo(item.project)} PR {prLabel(item.pr ?? '')}
        </DialogTitle>

        <div className="mb-4">
          <p className="text-body truncate text-text-2" title={item.title}>
            {item.title}
          </p>
          {item.branch && (
            <p className="text-code mt-1 truncate text-text-3" title={item.branch}>
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
          <p className="text-caption mb-4 text-text-3">Captain will check CI and squash merge</p>
        )}

        <div className="flex justify-end gap-2">
          {!isDone && (
            <>
              <button
                onClick={onCancel}
                disabled={pending}
                className="rounded-[6px] px-5 py-2 text-[13px] font-medium disabled:opacity-50"
                style={{
                  background: 'transparent',
                  color: 'var(--color-text-2)',
                  border: '1px solid var(--color-border)',
                }}
              >
                Cancel
              </button>
              <button
                onClick={() => onConfirm(item.id, item.pr ?? '', item.project ?? '')}
                disabled={pending || !!result}
                className="rounded-[6px] px-5 py-2 text-[13px] font-semibold disabled:opacity-50"
                style={{
                  background: 'var(--color-accent)',
                  color: 'var(--color-bg)',
                  fontWeight: 600,
                }}
              >
                {pending ? 'Starting...' : 'Merge'}
              </button>
            </>
          )}
          {result && !result.ok && (
            <button
              onClick={onCancel}
              className="rounded-[6px] px-5 py-2 text-[13px] font-medium text-text-2"
            >
              Dismiss
            </button>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
