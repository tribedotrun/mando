import React from 'react';
import { Button } from '#renderer/global/ui/primitives/button';

export function OutlineButton({
  onClick,
  disabled,
  children,
}: {
  onClick: () => void;
  disabled?: boolean;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <Button variant="outline" onClick={onClick} disabled={disabled} className="shrink-0">
      {children}
    </Button>
  );
}
