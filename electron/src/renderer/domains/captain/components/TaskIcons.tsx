import { Archive, ArchiveRestore, MoreHorizontal } from 'lucide-react';
import React from 'react';
import { Button } from '#renderer/components/ui/button';
import { MergeIcon } from '#renderer/global/components/icons';

export function MoreIcon() {
  return <MoreHorizontal size={16} />;
}

export function ArchiveBtn({
  onClick,
  pending,
  unarchive,
}: {
  onClick: () => void;
  pending?: boolean;
  unarchive?: boolean;
}): React.ReactElement {
  const Icon = unarchive ? ArchiveRestore : Archive;
  const label = unarchive ? 'Unarchive' : 'Archive';
  const testId = unarchive ? 'unarchive-btn' : 'archive-btn';
  return (
    <Button
      data-testid={testId}
      variant="outline"
      size="icon-xs"
      onClick={onClick}
      disabled={pending}
      aria-label={label}
      title={label}
    >
      <Icon size={16} />
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
