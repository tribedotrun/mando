import React from 'react';
import { Button } from '#renderer/global/ui/primitives/button';
import { MergeIcon } from '#renderer/global/ui/primitives/icons';

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
