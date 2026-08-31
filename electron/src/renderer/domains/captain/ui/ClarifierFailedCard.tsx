import React, { useCallback } from 'react';
import { formatEventTime } from '#renderer/domains/captain/service/feedHelpers';
import { Button } from '#renderer/global/ui/primitives/button';
import type { TimelineEvent, ItemStatus } from '#renderer/global/types';
import { InlineMarkdown } from '#renderer/global/ui/InlineMarkdown';
import { AlertTriangle } from 'lucide-react';
import log from '#renderer/global/service/logger';

function ClarifierFailedBlock({
  event,
  apiErrorStatus,
  message,
  onReanswer,
}: {
  event: TimelineEvent;
  // PR #889: sentinel 0 == non-HTTP error.
  apiErrorStatus: number;
  message: string;
  // Undefined when the task is not in `needs-clarification` and there's no
  // answer form to focus — the banner becomes purely informational.
  onReanswer?: () => void;
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
        <span className="text-body font-medium text-destructive">
          {onReanswer ? 'Agent errored — retry' : 'Agent errored'}
        </span>
        <span className="text-caption text-text-3">{time}</span>
        <span className="text-caption text-text-3">({statusLabel})</span>
      </div>
      {message ? (
        <div className="mb-2 break-words text-body text-text-1 [overflow-wrap:anywhere]">
          <InlineMarkdown text={message} />
        </div>
      ) : null}
      {onReanswer ? (
        <Button onClick={onReanswer} size="sm" variant="destructive">
          Re-answer
        </Button>
      ) : null}
    </div>
  );
}

export function ClarifierFailedRow({
  taskStatus,
  event,
  payload,
}: {
  taskStatus: ItemStatus;
  event: TimelineEvent;
  payload: ClarifierFailedPayload;
}): React.ReactElement {
  const onReanswer = useCallback(() => {
    const textarea = document.querySelector<HTMLTextAreaElement>(
      '[data-clarifier-target="answer"]',
    );
    if (!textarea) {
      log.warn('clarifier re-answer target not found in DOM (form likely hidden)');
      return;
    }
    textarea.scrollIntoView({ behavior: 'smooth', block: 'center' });
    textarea.focus({ preventScroll: true });
  }, []);
  return (
    <ClarifierFailedBlock
      event={event}
      apiErrorStatus={payload.api_error_status}
      message={payload.message}
      onReanswer={taskStatus === 'needs-clarification' ? onReanswer : undefined}
    />
  );
}

export interface ClarifierFailedPayload {
  event_type: 'clarifier_failed';
  // PR #889: sentinel "" == no agent session established (pre-prompt failure).
  session_id: string;
  // PR #889: sentinel 0 == non-HTTP error (transport/internal).
  api_error_status: number;
  message: string;
}
