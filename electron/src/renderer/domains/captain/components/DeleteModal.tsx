import React, { useState } from 'react';
import type { TaskItem } from '#renderer/types';
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogCancel,
  AlertDialogAction,
} from '#renderer/components/ui/alert-dialog';
import { Checkbox } from '#renderer/components/ui/checkbox';

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
  const hasPr = safe.some((b) => b.pr_number);

  const [closePr, setClosePr] = useState(false);

  return (
    <AlertDialog open={true} onOpenChange={() => onCancel()}>
      <AlertDialogContent data-testid="delete-modal" className="max-h-[80vh] overflow-y-auto">
        <AlertDialogHeader>
          <AlertDialogTitle>Delete Items</AlertDialogTitle>
          <AlertDialogDescription asChild>
            <div>
              <ul className="mb-3 max-h-[180px] list-none space-y-1 overflow-y-auto p-0">
                {safe.map((b) => (
                  <li key={b.id} className="truncate py-0.5 text-[13px] text-muted-foreground">
                    {b.title}
                  </li>
                ))}
                {inProgress.map((b) => (
                  <li key={b.id} className="truncate py-0.5 text-[13px] text-destructive">
                    {b.title} (in-progress -- skipped)
                  </li>
                ))}
              </ul>
            </div>
          </AlertDialogDescription>
        </AlertDialogHeader>

        {hasPr && (
          <div className="mb-3 flex flex-col gap-2 rounded-md bg-secondary px-3 py-2">
            <label className="flex cursor-pointer items-center gap-2 text-[13px]">
              <Checkbox
                checked={closePr}
                onCheckedChange={(checked) => setClosePr(checked === true)}
              />
              <span className="text-muted-foreground">Close associated PRs</span>
            </label>
          </div>
        )}

        {error && (
          <p className="mb-2 rounded-md bg-destructive/10 px-3 py-2 text-[13px] text-destructive">
            {error}
          </p>
        )}

        <AlertDialogFooter>
          <AlertDialogCancel onClick={onCancel}>Cancel</AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            onClick={() => onConfirm({ close_pr: closePr })}
            disabled={!canDelete || deleting}
          >
            {deleting ? 'Deleting...' : `Delete ${safe.length} item${safe.length !== 1 ? 's' : ''}`}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
