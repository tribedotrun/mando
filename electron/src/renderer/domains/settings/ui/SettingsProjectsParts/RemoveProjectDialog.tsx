import React from 'react';
import { Button } from '#renderer/global/ui/primitives/button';
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogCancel,
} from '#renderer/global/ui/primitives/alert-dialog';
import type { ProjectConfig } from '#renderer/global/types';

interface RemoveProjectDialogProps {
  project: ProjectConfig;
  isPending: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function RemoveProjectDialog({
  project,
  isPending,
  onConfirm,
  onCancel,
}: RemoveProjectDialogProps): React.ReactElement {
  return (
    <AlertDialog open onOpenChange={onCancel}>
      <AlertDialogContent size="sm">
        <AlertDialogHeader>
          <AlertDialogTitle>Remove project</AlertDialogTitle>
          <AlertDialogDescription>
            Remove {project.name}? All tasks belonging to this project will be deleted.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={isPending}>Cancel</AlertDialogCancel>
          <Button variant="destructive" size="sm" disabled={isPending} onClick={onConfirm}>
            {isPending ? 'Removing...' : 'Remove'}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
