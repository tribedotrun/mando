import React from 'react';
import { ClarificationTab } from '#renderer/domains/captain/ui/ClarificationTab';
import type { ClarifierQuestion, TimelineEvent } from '#renderer/global/types';
import {
  formatEventTime,
  EVENT_ICON_MAP,
  confidenceIconOverride,
  confidencePreview,
  getNudgeReason,
} from '#renderer/domains/captain/service/feedHelpers';
import { InlineMarkdown } from '#renderer/global/ui/InlineMarkdown';
import { MessageSquare, AlertTriangle } from 'lucide-react';
import { PrMarkdown } from '#renderer/global/ui/PrMarkdown';
import { StatusIndicator } from '#renderer/global/ui/StatusIndicator';

export function ActiveClarificationBlock({
  taskId,
  questions,
}: {
  taskId: number;
  questions: ClarifierQuestion[];
}): React.ReactElement {
  return (
    <div className="mx-3 my-2">
      <ClarificationTab taskId={taskId} questions={questions} />
    </div>
  );
}

export function ClarificationSummaryBlock({
  event,
  questions,
}: {
  event: TimelineEvent;
  questions: ClarifierQuestion[];
}): React.ReactElement {
  const time = formatEventTime(event.timestamp);

  return (
    <div
      className="mx-3 my-2 rounded-lg px-4 py-3"
      style={{
        background: 'color-mix(in srgb, var(--needs-human) 6%, transparent)',
        border: '1px solid color-mix(in srgb, var(--needs-human) 20%, transparent)',
      }}
    >
      <div className="mb-2 flex items-center gap-2">
        <MessageSquare size={14} style={{ color: 'var(--needs-human)' }} />
        <span className="text-body font-medium" style={{ color: 'var(--needs-human)' }}>
          Clarification requested
        </span>
        <span className="text-caption text-text-3">{time}</span>
      </div>
      {questions.map((question, index) => (
        <div key={index} className="mb-1 text-body text-text-2">
          <span className="text-text-3">{index + 1}.</span>{' '}
          <InlineMarkdown text={question.question} />
          {question.self_answered && (
            <span className="ml-1 text-caption text-text-3">(auto-resolved)</span>
          )}
        </div>
      ))}
    </div>
  );
}

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
      <div className="break-words text-body text-text-1">
        <PrMarkdown text={report || event.summary} />
      </div>
    </div>
  );
}

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
