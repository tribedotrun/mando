import { useNavigate } from '@tanstack/react-router';
import { FolderX, RotateCw } from 'lucide-react';
import { useWorkbenchArchive } from '#renderer/domains/captain/runtime/hooks';
import { useWorktreeMissingActions } from '#renderer/domains/captain/terminal/runtime/useWorktreeMissingActions';
import { CardFrame } from '#renderer/domains/captain/ui/CardFrame';
import { Button } from '#renderer/global/ui/primitives/button';

interface WorktreeMissingProps {
  workbenchId: number;
  worktree: string;
}

export function WorktreeMissing({ workbenchId, worktree }: WorktreeMissingProps) {
  const navigate = useNavigate();
  // useWorkbenchArchive is the feedback-wrapped variant — it already toasts
  // on error via useMutationFeedback, so the callsite only adds onSuccess.
  const archive = useWorkbenchArchive();
  const { refresh, retrying } = useWorktreeMissingActions();

  const handleArchive = () => {
    archive.mutate(
      { id: workbenchId },
      {
        onSuccess: () => void navigate({ to: '/', replace: true }),
      },
    );
  };

  return (
    <div className="flex h-full items-center justify-center px-6">
      <CardFrame color="var(--destructive)" className="w-full max-w-md !flex-col !items-stretch">
        <div className="flex items-center gap-2">
          <FolderX size={16} className="text-destructive" />
          <span className="text-body font-medium text-destructive">Worktree no longer exists</span>
        </div>
        <p className="text-caption text-text-2">
          The git worktree for this workbench was removed. Recreate it on disk and click Retry, or
          archive the workbench.
        </p>
        <div
          className="rounded px-2 py-1 font-mono text-caption text-text-3"
          style={{ background: 'var(--color-surface-2)' }}
        >
          {worktree || '(no path on record)'}
        </div>
        <div className="flex justify-end gap-2">
          <Button size="sm" variant="outline" onClick={() => void refresh()} disabled={retrying}>
            <RotateCw size={12} className={retrying ? 'animate-spin' : undefined} />
            {retrying ? 'Checking…' : 'Retry'}
          </Button>
          <Button
            size="sm"
            variant="destructive"
            onClick={handleArchive}
            disabled={archive.isPending}
          >
            {archive.isPending ? 'Archiving…' : 'Archive workbench'}
          </Button>
        </div>
      </CardFrame>
    </div>
  );
}
