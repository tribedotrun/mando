import React from 'react';
import { formatEventTime } from '#renderer/domains/captain/service/feedHelpers';
import { RetryButton } from '#renderer/domains/captain/ui/RetryButton';
import { useClarifierRetry } from '#renderer/domains/captain/runtime/useClarifierRetry';
import type { TimelineEvent } from '#renderer/global/types';
import { AlertTriangle } from 'lucide-react';

export interface ClarifierFailedPayload {
  event_type: 'clarifier_failed';
  // PR #889: sentinel "" == no CC session established (pre-prompt failure).
  session_id: string;
  // PR #889: sentinel 0 == non-HTTP error (transport/internal).
  api_error_status: number;
  message: string;
}

export function ClarifierFailedRow({
  taskId,
  event,
  payload,
}: {
  taskId: number;
  event: TimelineEvent;
  payload: ClarifierFailedPayload;
}): React.ReactElement {
  const onRetry = useClarifierRetry(taskId);
  return (
    <ClarifierFailedBlock
      event={event}
      apiErrorStatus={payload.api_error_status}
      message={payload.message}
      onRetry={onRetry}
    />
  );
}

export function ClarifierFailedBlock({
  event,
  apiErrorStatus,
  message,
  onRetry,
}: {
  event: TimelineEvent;
  // PR #889: sentinel 0 == non-HTTP error.
  apiErrorStatus: number;
  message: string;
  onRetry: () => Promise<unknown> | void;
}): React.ReactElement {
  const time = formatEventTime(event.timestamp);
  const statusLabel = apiErrorStatus > 0 ? `status ${apiErrorStatus}` : 'no status';
  return (
    <div
      className="mx-3 my-2 rounded-lg px-4 py-3"
      style={{
        background: 'color-mix(in srgb, var(--destructive) 6%, transparent)',
        border: '1px solid color-mix(in srgb, var(--destructive) 20%, transparent)',
      }}
    >
      <div className="mb-2 flex items-center gap-2">
        <AlertTriangle size={14} className="text-destructive" />
        <span className="text-body font-medium text-destructive">CC errored — retry</span>
        <span className="text-caption text-text-3">{time}</span>
        <span className="text-caption text-text-3">({statusLabel})</span>
      </div>
      {message ? (
        <p className="mb-2 break-words text-body text-text-1 [overflow-wrap:anywhere]">{message}</p>
      ) : null}
      <RetryButton
        onRetry={onRetry}
        label="Refresh and re-answer"
        retryingLabel="Refreshing\u2026"
        size="sm"
        variant="destructive"
      />
    </div>
  );
}
