import React, { useCallback } from 'react';
import type { TaskItem } from '#renderer/types';
import { prLabel, shortRepo } from '#renderer/utils';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '#renderer/components/ui/dialog';
import { Button } from '#renderer/components/ui/button';

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
      <DialogContent data-testid="merge-modal" showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>
            Merge {shortRepo(item.project)} PR {prLabel(item.pr ?? '')}
          </DialogTitle>
          <DialogDescription className="truncate" title={item.title}>
            {item.title}
          </DialogDescription>
        </DialogHeader>

        {item.branch && (
          <p className="text-code truncate text-muted-foreground" title={item.branch}>
            {item.branch}
          </p>
        )}

        {result ? (
          <div
            className={`rounded-md px-3 py-2 text-body ${result.ok ? 'bg-success-bg text-success' : 'bg-destructive-bg text-destructive'}`}
          >
            {result.message}
          </div>
        ) : (
          <p className="text-caption text-muted-foreground">
            Captain will check CI and squash merge
          </p>
        )}

        <DialogFooter>
          {!isDone && (
            <>
              <Button variant="outline" onClick={onCancel} disabled={pending}>
                Cancel
              </Button>
              <Button
                onClick={() => onConfirm(item.id, item.pr ?? '', item.project ?? '')}
                disabled={pending || !!result}
              >
                {pending ? 'Starting...' : 'Merge'}
              </Button>
            </>
          )}
          {result && !result.ok && (
            <Button variant="ghost" onClick={onCancel}>
              Dismiss
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
