import React from 'react';
import type {
  SystemApiRetryEvent,
  SystemCompactBoundaryEvent,
  SystemHookEvent,
  SystemInitEvent,
  SystemLocalCommandOutputEvent,
  SystemRateLimitEvent,
  SystemStatusEvent,
  UnknownEvent,
} from '#renderer/global/types';

type SystemEventPayload =
  | { kind: 'init'; data: SystemInitEvent; isBoundary: boolean }
  | { kind: 'compact'; data: SystemCompactBoundaryEvent }
  | { kind: 'status'; data: SystemStatusEvent }
  | { kind: 'retry'; data: SystemApiRetryEvent }
  | { kind: 'local'; data: SystemLocalCommandOutputEvent }
  | { kind: 'hook'; data: SystemHookEvent }
  | { kind: 'ratelimit'; data: SystemRateLimitEvent }
  | { kind: 'unknown'; data: UnknownEvent };

export function SystemMessage({ event }: { event: SystemEventPayload }): React.ReactElement | null {
  switch (event.kind) {
    case 'init':
      return (
        <div className="border-y border-muted/60 py-2 text-label text-muted-foreground">
          <span className="uppercase tracking-wider">
            {event.isBoundary ? '✻ session resumed' : '✻ session start'}
          </span>
          {event.data.model && <span className="ml-2">· {event.data.model}</span>}
          {event.data.cwd && <span className="ml-2 opacity-70">· {event.data.cwd}</span>}
        </div>
      );
    case 'compact':
      return (
        <div className="flex items-center gap-2 border-y border-muted/60 py-2 text-label italic text-muted-foreground">
          <span>✻ context compacted</span>
          {event.data.reason && <span className="opacity-70">· {event.data.reason}</span>}
        </div>
      );
    case 'status':
      return (
        <div className="py-1 text-label italic text-muted-foreground">
          {event.data.status ?? 'status'} {event.data.message ? `· ${event.data.message}` : ''}
        </div>
      );
    case 'retry':
      return (
        <div className="py-1 text-label italic text-destructive/80">
          api retry {event.data.attempt ? `#${event.data.attempt}` : ''}
          {event.data.message ? ` — ${event.data.message}` : ''}
        </div>
      );
    case 'local':
      return (
        <pre className="mt-1 max-h-32 overflow-auto rounded bg-muted/40 px-3 py-2 text-label text-muted-foreground">
          {event.data.command ? `$ ${event.data.command}\n` : ''}
          {event.data.output}
        </pre>
      );
    case 'hook':
      return null;
    case 'ratelimit':
      return (
        <div className="py-1 text-label italic text-amber-600 dark:text-amber-400">
          ⚠ rate limit signal
        </div>
      );
    case 'unknown':
      return (
        <div className="py-1 text-label italic text-muted-foreground opacity-60">
          unknown event {event.data.rawType ? `(${event.data.rawType})` : ''}
        </div>
      );
  }
}
