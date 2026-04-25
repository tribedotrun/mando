import { Archive } from 'lucide-react';
import React from 'react';
import { Button } from '#renderer/global/ui/primitives/button';

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
