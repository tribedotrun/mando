import React from 'react';
import type { SessionStatus } from '#renderer/global/types';

const STATUS_DOT_CLASS: Record<SessionStatus, string> = Object.freeze({
  running: 'bg-success',
  stopped: 'bg-text-3',
  failed: 'bg-destructive',
});

export function SessionDot({ status }: { status?: SessionStatus }): React.ReactElement {
  const bgClass = status ? STATUS_DOT_CLASS[status] : 'bg-text-4';
  return (
    <span
      className={`inline-block size-2 shrink-0 rounded-full ${bgClass}${status === 'running' ? ' animate-pulse' : ''}`}
    />
  );
}
