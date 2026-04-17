import React, { useState } from 'react';
import type { TaskItem } from '#renderer/global/types';
import { prLabel, shortRepo } from '#renderer/global/service/utils';
import {
  Dialog,
  DialogContentPlain,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '#renderer/global/ui/dialog';
import { Button } from '#renderer/global/ui/button';

interface Props {
  item: TaskItem;
  onConfirm: (itemId: number, prNumber: number, project: string) => void;
  onCancel: () => void;
}

export function MergeModal({ item, onConfirm, onCancel }: Props): React.ReactElement {
  const [confirmed, setConfirmed] = useState(false);

  return (
    <Dialog open={true} onOpenChange={onCancel}>
      <DialogContentPlain data-testid="merge-modal" className="overflow-hidden">
        <DialogHeader className="min-w-0">
          <DialogTitle
            className="truncate"
            title={`Merge ${shortRepo(item.project)} PR ${item.pr_number ? prLabel(item.pr_number) : ''}`}
          >
            Merge {shortRepo(item.project)} PR {item.pr_number ? prLabel(item.pr_number) : ''}
          </DialogTitle>
          <DialogDescription className="truncate" title={item.title}>
            {item.title}
          </DialogDescription>
        </DialogHeader>

        {item.branch && (
          <p className="min-w-0 truncate text-code text-muted-foreground" title={item.branch}>
            {item.branch}
          </p>
        )}

        <p className="text-caption text-muted-foreground">Captain will check CI and squash merge</p>

        <DialogFooter>
          <Button variant="outline" onClick={onCancel}>
            Cancel
          </Button>
          <Button
            disabled={confirmed}
            onClick={() => {
              setConfirmed(true);
              onConfirm(item.id, item.pr_number ?? 0, item.project ?? '');
            }}
          >
            Merge
          </Button>
        </DialogFooter>
      </DialogContentPlain>
    </Dialog>
  );
}
