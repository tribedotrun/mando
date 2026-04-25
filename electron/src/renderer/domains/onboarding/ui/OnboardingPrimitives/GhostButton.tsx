import React from 'react';
import { Button } from '#renderer/global/ui/primitives/button';

export function GhostButton({
  onClick,
  children,
}: {
  onClick: () => void;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <Button variant="ghost" onClick={onClick}>
      {children}
    </Button>
  );
}
