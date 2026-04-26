import React from 'react';
import { formatEventTime } from '#renderer/domains/captain/service/feedHelpers';
import type { TimelineEvent, ClarifierQuestion } from '#renderer/global/types';
import { InlineMarkdown } from '#renderer/global/ui/InlineMarkdown';
import { MessageSquare } from 'lucide-react';

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
