import { Archive, MoreHorizontal } from 'lucide-react';
import React from 'react';
import { Button } from '#renderer/global/ui/button';
import { MergeIcon } from '#renderer/global/ui/icons';

export function MoreIcon() {
  return <MoreHorizontal size={16} />;
}

export function ArchiveBtn({
  onClick,
  pending,
}: {
  onClick: () => void;
  pending?: boolean;
}): React.ReactElement {
  return (
    <Button
      data-testid="archive-btn"
      variant="outline"
      size="icon-xs"
      onClick={onClick}
      disabled={pending}
      aria-label="Archive"
      title="Archive"
    >
      <Archive size={16} />
    </Button>
  );
}

export function MergeBtn({ onClick }: { onClick: () => void }): React.ReactElement {
  return (
    <Button
      data-testid="merge-btn"
      variant="ghost"
      size="icon-xs"
      onClick={onClick}
      className="bg-success-bg text-success"
    >
      <MergeIcon />
    </Button>
  );
}
