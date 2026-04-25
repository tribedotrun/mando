import React, { useState } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { formatEventTime } from '#renderer/domains/captain/service/feedHelpers';
import { StatusIndicator } from '#renderer/global/ui/StatusIndicator';
import type { TimelineEvent } from '#renderer/global/types';

interface PlanSummaryBlockProps {
  event: TimelineEvent;
  status: 'plan-ready' | 'completed-no-pr';
  title: string;
  action?: React.ReactNode;
}

export function PlanSummaryBlock({
  event,
  status,
  title,
  action,
}: PlanSummaryBlockProps): React.ReactElement {
  const [planOpen, setPlanOpen] = useState(false);
  const diagram = event.data.event_type === 'plan_completed' ? event.data.diagram : '';
  const plan = event.data.event_type === 'plan_completed' ? event.data.plan : '';
  const time = formatEventTime(event.timestamp);

  return (
    <div className="mx-3 my-2 space-y-3 rounded-lg bg-muted/40 px-4 py-3">
      <div className="flex items-center gap-2">
        <StatusIndicator status={status} />
        <span className="text-body font-medium text-text-1">{title}</span>
        <span className="text-caption text-text-3">{time}</span>
      </div>
      {diagram && (
        <pre className="overflow-x-auto rounded-md bg-muted px-3 py-2 font-mono text-caption text-foreground">
          {diagram}
        </pre>
      )}
      {plan && (
        <div>
          <button
            type="button"
            className="flex items-center gap-1 text-caption font-medium text-muted-foreground hover:text-foreground"
            onClick={() => setPlanOpen(!planOpen)}
          >
            {planOpen ? (
              <ChevronDown className="size-3.5" />
            ) : (
              <ChevronRight className="size-3.5" />
            )}
            Full plan
          </button>
          {planOpen && (
            <pre className="mt-1 overflow-x-auto whitespace-pre-wrap rounded-md bg-muted px-3 py-2 text-caption text-muted-foreground">
              {plan}
            </pre>
          )}
        </div>
      )}
      {action}
    </div>
  );
}
