import React from 'react';
import { AlertTriangle } from 'lucide-react';
import type {
  SystemApiRetryEvent,
  SystemCompactBoundaryEvent,
  SystemHookEvent,
  SystemInitEvent,
  SystemLocalCommandOutputEvent,
  SystemRateLimitEvent,
  SystemStatusEvent,
  SystemThinkingTokensEvent,
  UnknownEvent,
} from '#renderer/global/types';
import {
  parseClaudeRateLimitInfo,
  unknownEventTitle,
} from '#renderer/domains/sessions/service/transcriptEvents';
import { prettyJson } from '#renderer/domains/sessions/service/transcriptRenderHelpers';

type SystemEventPayload =
  | { kind: 'init'; data: SystemInitEvent; isBoundary: boolean }
  | { kind: 'compact'; data: SystemCompactBoundaryEvent }
  | { kind: 'status'; data: SystemStatusEvent }
  | { kind: 'retry'; data: SystemApiRetryEvent }
  | { kind: 'local'; data: SystemLocalCommandOutputEvent }
  | { kind: 'hook'; data: SystemHookEvent }
  | { kind: 'ratelimit'; data: SystemRateLimitEvent }
  | { kind: 'thinking_tokens'; data: SystemThinkingTokensEvent }
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
        <div className="flex items-start gap-2 rounded px-2 py-1.5 text-label text-amber-700 dark:text-amber-300">
          <AlertTriangle className="mt-0.5 shrink-0 opacity-70" size={13} />
          <span className="font-medium uppercase tracking-wider">
            {event.data.status ?? 'status'}
          </span>
          {event.data.message && (
            <span className="min-w-0 text-muted-foreground">{event.data.message}</span>
          )}
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
    case 'thinking_tokens':
      // CC progress signal — estimated token counts + ids only, no thinking
      // text. Real thinking arrives as `assistant.thinking` blocks and
      // renders via `ThinkingBlock`. Suppress like `hook` (raw JSONL still
      // reachable via the "Open JSONL" button).
      return null;
    case 'ratelimit':
      return <ClaudeRateLimitMessage event={event.data} />;
    case 'unknown':
      return (
        <details className="rounded-md border border-border/50 bg-muted/20 px-3 py-2 text-label text-muted-foreground">
          <summary className="cursor-pointer select-none uppercase tracking-wider text-text-3">
            {unknownEventTitle(event.data)}
          </summary>
          <pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap rounded bg-background/70 px-3 py-2 font-mono text-[11px] leading-4 text-muted-foreground">
            {prettyJson(event.data.raw)}
          </pre>
        </details>
      );
  }
}

function ClaudeRateLimitMessage({ event }: { event: SystemRateLimitEvent }): React.ReactElement {
  const info = parseClaudeRateLimitInfo(event.info);
  if (!info) {
    return (
      <details className="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-label">
        <summary className="cursor-pointer text-destructive">
          Rate limit payload unavailable
        </summary>
        <pre className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap font-mono text-[11px] text-muted-foreground">
          {prettyJson(event.info)}
        </pre>
      </details>
    );
  }
  const isRejected = info.status === 'rejected';
  const primary = info.windows.find((window) => window.name === info.rateLimitType);
  const summaryUtilization = formatUtilization(primary?.utilization ?? null);
  const summaryReset = formatReset(primary?.resetsAt ?? info.resetsAt);
  return (
    <details
      className={`rounded-md border px-3 py-2 text-label ${
        isRejected
          ? 'border-destructive/35 bg-destructive/5'
          : 'border-border/60 bg-muted/15 text-muted-foreground'
      }`}
    >
      <summary className="cursor-pointer select-none">
        <span className="font-medium uppercase tracking-wider">
          Claude quota · {formatLabel(info.status)}
        </span>
        {info.rateLimitType && <span> · {formatLabel(info.rateLimitType)}</span>}
        {summaryUtilization && <span> · {summaryUtilization} used</span>}
        {summaryReset && <span> · resets {summaryReset}</span>}
      </summary>
      <div className="mt-2 grid gap-1 border-t border-border/50 pt-2 sm:grid-cols-2">
        {info.windows.map((window) => (
          <div key={window.name} className="flex items-baseline justify-between gap-3">
            <span>{formatLabel(window.name)}</span>
            <span className="font-mono text-text-3">
              {formatUtilization(window.utilization) ?? '—'}
              {formatReset(window.resetsAt) ? ` · ${formatReset(window.resetsAt)}` : ''}
            </span>
          </div>
        ))}
      </div>
      <div className="mt-2 text-text-3">
        Overage {info.overageStatus ? formatLabel(info.overageStatus) : 'unknown'}
        {info.overageDisabledReason
          ? ` · ${formatLabel(info.overageDisabledReason)}`
          : info.isUsingOverage
            ? ' · active'
            : ' · inactive'}
      </div>
    </details>
  );
}

function formatLabel(value: string): string {
  return value.replaceAll('_', ' ');
}

function formatUtilization(value: number | null): string | null {
  if (value === null) return null;
  return new Intl.NumberFormat(undefined, {
    style: 'percent',
    maximumFractionDigits: 1,
  }).format(value);
}

function formatReset(value: number | null): string | null {
  if (value === null) return null;
  return new Intl.DateTimeFormat(undefined, {
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  }).format(new Date(value * 1000));
}
