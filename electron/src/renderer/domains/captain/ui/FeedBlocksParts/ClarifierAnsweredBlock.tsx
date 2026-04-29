import React from 'react';
import { formatEventTime } from '#renderer/domains/captain/service/feedHelpers';
import { PrMarkdown } from '#renderer/global/ui/PrMarkdown';
import { StatusIndicator } from '#renderer/global/ui/StatusIndicator';
import type { TimelineEvent } from '#renderer/global/types';

export function ClarifierAnsweredBlock({
  event,
  taskContext,
}: {
  event: TimelineEvent;
  taskContext: string;
}): React.ReactElement {
  const time = formatEventTime(event.timestamp);
  const body = taskContext.trim();

  return (
    <div
      className="mx-3 my-2 space-y-2 rounded-lg bg-muted/40 px-4 py-3"
      data-testid="clarifier-answered-block"
    >
      <div className="flex items-center gap-2">
        <StatusIndicator status="completed-no-pr" />
        <span className="text-body font-medium text-text-1">Answered</span>
        <span className="text-caption text-text-3">{time}</span>
      </div>
      {body ? (
        <div className="break-words text-body text-text-1 [overflow-wrap:anywhere]">
          <PrMarkdown text={body} />
        </div>
      ) : (
        <div className="text-caption text-text-3">{event.summary}</div>
      )}
    </div>
  );
}
