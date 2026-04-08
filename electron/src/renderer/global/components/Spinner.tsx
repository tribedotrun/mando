import React from 'react';

interface SpinnerProps {
  size?: number;
  color?: string;
  borderWidth?: number;
}

export function Spinner({
  size = 14,
  color = 'var(--muted-foreground)',
  borderWidth = 2,
}: SpinnerProps): React.ReactElement {
  return (
    <span
      className="inline-block shrink-0 animate-spin rounded-full"
      style={{
        width: size,
        height: size,
        border: `${borderWidth}px solid ${color}`,
        borderTopColor: 'transparent',
      }}
    />
  );
}
