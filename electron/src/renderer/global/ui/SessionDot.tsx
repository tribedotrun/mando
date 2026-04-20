import React from 'react';

const STATUS_DOT_CLASS: Record<string, string> = Object.freeze({
  running: 'bg-success',
  stopped: 'bg-text-3',
  failed: 'bg-destructive',
});

export function SessionDot({ status }: { status?: string }): React.ReactElement {
  const bgClass = STATUS_DOT_CLASS[status ?? ''] ?? 'bg-text-4';
  return (
    <span
      className={`inline-block size-2 shrink-0 rounded-full ${bgClass}${status === 'running' ? ' animate-pulse' : ''}`}
    />
  );
}
