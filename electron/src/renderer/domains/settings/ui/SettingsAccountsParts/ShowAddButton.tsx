import React from 'react';
import { Plus } from 'lucide-react';
import { Button } from '#renderer/global/ui/primitives/button';

export function ShowAddButton({ onClick }: { onClick: () => void }): React.ReactElement {
  return (
    <Button variant="outline" size="sm" onClick={onClick} className="gap-2">
      <Plus size={14} />
      Add Setup Token
    </Button>
  );
}
