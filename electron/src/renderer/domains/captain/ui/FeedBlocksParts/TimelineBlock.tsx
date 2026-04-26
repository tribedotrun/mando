import React from 'react';
import {
  EVENT_ICON_MAP,
  confidenceIconOverride,
  confidencePreview,
  formatEventTime,
  getNudgeReason,
} from '#renderer/domains/captain/service/feedHelpers';
import type { TimelineEvent } from '#renderer/global/types';
import { StatusIndicator } from '#renderer/global/ui/StatusIndicator';
import { PrMarkdown } from '#renderer/global/ui/PrMarkdown';
import { InlineMarkdown } from '#renderer/global/ui/InlineMarkdown';

export function TimelineBlock({ event }: { event: TimelineEvent }) {
  const iconStatus =
    confidenceIconOverride(event) ?? EVENT_ICON_MAP[event.data.event_type] ?? 'queued';
  const time = formatEventTime(event.timestamp);
  const nudgeReason = getNudgeReason(event);
  const triageDetail = confidencePreview(event);

  return (
    <div className="flex items-start gap-3 px-3 py-2">
      <div className="mt-0.5 flex-shrink-0">
        <StatusIndicator status={iconStatus} />
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <span className="text-caption text-text-2">{time}</span>
          <span className="max-w-[120px] truncate text-caption font-medium text-text-2">
            {event.actor}
          </span>
        </div>
        <div className="break-words text-body text-text-1">
          <PrMarkdown text={event.summary} />
        </div>
        {nudgeReason ? (
          <div className="mt-0.5 text-caption text-text-3 [overflow-wrap:anywhere]">
            Reason: <InlineMarkdown text={nudgeReason} />
          </div>
        ) : null}
        {triageDetail ? (
          <div className="mt-0.5 text-caption text-text-3 [overflow-wrap:anywhere]">
            Confidence: {triageDetail.confidence}
            {triageDetail.reason ? (
              <>
                {' — '}
                <InlineMarkdown text={triageDetail.reason} />
              </>
            ) : null}
          </div>
        ) : null}
      </div>
    </div>
  );
}
