import React, { useState } from 'react';
import { Button } from '#renderer/components/ui/button';

interface Props {
  onRetry: () => Promise<unknown> | void;
  label?: string;
  retryingLabel?: string;
  className?: string;
  variant?: 'default' | 'outline' | 'ghost' | 'secondary' | 'destructive' | 'link';
  size?: 'default' | 'xs' | 'sm' | 'lg';
}

/**
 * Button that calls `onRetry` and swaps to a disabled "retrying" state until
 * the returned promise settles. Used for the disconnected banner retry and
 * the init-error retry. Callers typically reload or navigate after retry,
 * so the disabled state persists until unmount.
 */
export function RetryButton({
  onRetry,
  label = 'Retry',
  retryingLabel = 'Retrying\u2026',
  className,
  variant = 'default',
  size = 'default',
}: Props): React.ReactElement {
  const [retrying, setRetrying] = useState(false);
  return (
    <Button
      variant={variant}
      size={size}
      className={className}
      disabled={retrying}
      onClick={() => {
        setRetrying(true);
        const result = onRetry();
        if (result instanceof Promise) {
          void result.finally(() => setRetrying(false));
        }
      }}
    >
      {retrying ? retryingLabel : label}
    </Button>
  );
}
