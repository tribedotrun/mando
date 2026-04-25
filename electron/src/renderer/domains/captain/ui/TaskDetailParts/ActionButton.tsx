import React from 'react';
import { Button } from '#renderer/global/ui/primitives/button';

export function ActionButton({
  label,
  onClick,
  accent,
}: {
  label: string;
  onClick: () => void;
  accent?: boolean;
}): React.ReactElement {
  return (
    <Button variant={accent ? 'default' : 'outline'} size="sm" onClick={onClick}>
      {label}
    </Button>
  );
}
