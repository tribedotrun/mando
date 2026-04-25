import React from 'react';
import { Button } from '#renderer/global/ui/primitives/button';

export function PrimaryButton({
  onClick,
  disabled,
  children,
}: {
  onClick: () => void;
  disabled?: boolean;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <Button onClick={onClick} disabled={disabled}>
      {children}
    </Button>
  );
}
