import React from 'react';
import { Button } from '#renderer/global/ui/primitives/button';

export function ActionBtn({
  label,
  onClick,
  testId,
  disabled,
  pending,
}: {
  label: string;
  onClick: () => void;
  testId?: string;
  disabled?: boolean;
  pending?: boolean;
}): React.ReactElement {
  const isDisabled = disabled || pending;
  return (
    <Button
      data-testid={testId}
      variant="outline"
      size="xs"
      onClick={onClick}
      disabled={isDisabled}
    >
      {pending ? '...' : label}
    </Button>
  );
}
