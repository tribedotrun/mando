import React from 'react';
import type { ResultEvent } from '#renderer/global/types';
import {
  formatCost,
  formatDuration,
  humanOutcome,
} from '#renderer/domains/sessions/service/transcriptRenderHelpers';

export function SessionFooter({ event }: { event: ResultEvent }): React.ReactElement {
  const { outcome, summary } = event;
  const isError = summary.isError;
  const totalCost =
    typeof summary.totalCostUsd === 'number' ? formatCost(summary.totalCostUsd) : '—';
  const duration =
    typeof summary.durationMs === 'number' ? formatDuration(summary.durationMs) : '—';
  const turns = summary.numTurns != null ? `${summary.numTurns} turns` : '—';

  return (
    <div
      className={`mt-4 rounded border px-4 py-3 text-label ${
        isError ? 'border-destructive/40 text-destructive' : 'border-muted text-muted-foreground'
      }`}
    >
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1">
        <span className="font-medium uppercase tracking-wider">
          {isError ? 'failed' : 'completed'}
        </span>
        <span>· {humanOutcome(outcome)}</span>
        <span>· {turns}</span>
        <span>· {duration}</span>
        <span>· {totalCost}</span>
        {summary.stopReason && <span>· {summary.stopReason}</span>}
      </div>
      {summary.permissionDenials.length > 0 && (
        <div className="mt-2 text-label text-destructive/90">
          {summary.permissionDenials.length} permission denial
          {summary.permissionDenials.length > 1 ? 's' : ''}
        </div>
      )}
      {summary.errors.length > 0 && (
        <ul className="mt-2 list-disc space-y-1 pl-4 text-destructive/90">
          {summary.errors.map((err, i) => (
            <li key={i}>{err}</li>
          ))}
        </ul>
      )}
      {summary.modelUsage.length > 0 && (
        <div className="mt-2 grid grid-cols-2 gap-x-4 gap-y-1 text-label opacity-80 md:grid-cols-3">
          {summary.modelUsage.map((m) => (
            <div key={m.model}>
              <span className="font-medium">{m.model}</span>
              <span className="ml-2 opacity-70">
                {m.usage.input_tokens + m.usage.output_tokens} tok
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
