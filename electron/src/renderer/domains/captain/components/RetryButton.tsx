import React, { useState } from 'react';

interface Props {
  onRetry: () => Promise<unknown> | void;
  label?: string;
  retryingLabel?: string;
  className?: string;
  style?: React.CSSProperties;
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
  style,
}: Props): React.ReactElement {
  const [retrying, setRetrying] = useState(false);
  return (
    <button
      className={className}
      style={{
        ...style,
        opacity: retrying ? 0.5 : 1,
        pointerEvents: retrying ? 'none' : undefined,
      }}
      onClick={() => {
        setRetrying(true);
        const result = onRetry();
        if (result instanceof Promise) {
          result.finally(() => setRetrying(false));
        }
      }}
    >
      {retrying ? retryingLabel : label}
    </button>
  );
}
