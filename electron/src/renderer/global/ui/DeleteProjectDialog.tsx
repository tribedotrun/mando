import React, { useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
} from '#renderer/global/ui/primitives/dialog';
import { Button } from '#renderer/global/ui/primitives/button';
import { Checkbox } from '#renderer/global/ui/primitives/checkbox';
import { toast } from 'sonner';
import { useSidebar } from '#renderer/global/runtime/SidebarContext';

interface DeleteProjectDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  name: string;
  count: number;
}

export function DeleteProjectDialog({ open, onOpenChange, name, count }: DeleteProjectDialogProps) {
  const { actions } = useSidebar();
  const [confirmed, setConfirmed] = useState(false);
  const [pending, setPending] = useState(false);
  const dialogTitle = count > 0 ? 'Delete project and tasks?' : 'Remove project?';

  const removeProject = async (): Promise<void> => {
    setPending(true);
    try {
      await actions.removeProject(name);
      onOpenChange(false);
      setConfirmed(false);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to delete project');
    } finally {
      setPending(false);
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(isOpen) => {
        if (!isOpen && !pending) {
          onOpenChange(false);
          setConfirmed(false);
        }
      }}
    >
      <DialogContent aria-label={dialogTitle}>
        <DialogTitle>{dialogTitle}</DialogTitle>
        <DialogDescription>
          {count > 0 ? (
            <>
              &ldquo;{name}&rdquo; and{' '}
              <strong className="text-muted-foreground">
                {count} {count === 1 ? 'task' : 'tasks'}
              </strong>{' '}
              belonging to it will be permanently deleted. Project files on disk are not affected.
            </>
          ) : (
            <>
              &ldquo;{name}&rdquo; will be removed from Mando. Project files on disk are not
              affected.
            </>
          )}
        </DialogDescription>

        {count > 0 && (
          <label className="mb-4 flex cursor-pointer items-center gap-2 text-[13px] text-muted-foreground">
            <Checkbox
              checked={confirmed}
              onCheckedChange={(checked) => setConfirmed(checked === true)}
            />
            I understand this cannot be undone
          </label>
        )}

        <div className="flex justify-end gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              onOpenChange(false);
              setConfirmed(false);
            }}
            disabled={pending}
          >
            Cancel
          </Button>
          <Button
            variant="destructive"
            size="sm"
            onClick={() => void removeProject()}
            disabled={(count > 0 && !confirmed) || pending}
          >
            {pending
              ? 'Deleting...'
              : count > 0
                ? `Delete project and ${count} ${count === 1 ? 'task' : 'tasks'}`
                : 'Remove'}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
