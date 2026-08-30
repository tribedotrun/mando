import React from 'react';
import { Archive } from 'lucide-react';
import { useWorkbenchArchive } from '#renderer/domains/captain/runtime/hooks';
import { Button } from '#renderer/global/ui/primitives/button';

interface WorkbenchWithoutTaskProps {
  workbenchId: number;
  onArchived: () => void;
}

/** What a workbench whose task is gone shows. Legacy workbenches created
 *  before every workbench carried a task land here with nothing to render and
 *  no way out except the sidebar, so the empty state owns the archive action
 *  that clears them. */
export function WorkbenchWithoutTask({
  workbenchId,
  onArchived,
}: WorkbenchWithoutTaskProps): React.ReactElement {
  const archive = useWorkbenchArchive();

  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 text-muted-foreground">
      <p>No task for this workbench</p>
      <Button
        variant="outline"
        size="sm"
        data-testid="workbench-archive-empty"
        disabled={archive.isPending}
        onClick={() => archive.mutate({ id: workbenchId }, { onSuccess: onArchived })}
      >
        <Archive className="size-3.5" />
        {archive.isPending ? 'Archiving…' : 'Archive workbench'}
      </Button>
    </div>
  );
}
