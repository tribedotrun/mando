import React from 'react';
import { formatEventTime } from '#renderer/domains/captain/service/feedHelpers';
import { PrMarkdown } from '#renderer/global/ui/PrMarkdown';
import type { TimelineEvent } from '#renderer/global/types';
import { AlertTriangle } from 'lucide-react';

export function EscalationBlock({
  event,
  report,
}: {
  event: TimelineEvent;
  report?: string | null;
}) {
  const time = formatEventTime(event.timestamp);

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
        <span className="text-body font-medium text-destructive">Escalated</span>
        <span className="text-caption text-text-3">{time}</span>
      </div>
      {report ? (
        <div className="text-body text-text-1">
          <PrMarkdown text={report} />
        </div>
      ) : (
        <p className="break-words text-body text-text-1">{event.summary}</p>
      )}
    </div>
  );
}
