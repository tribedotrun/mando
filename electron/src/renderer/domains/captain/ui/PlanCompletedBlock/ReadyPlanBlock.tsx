import React from 'react';
import { Loader2, Play } from 'lucide-react';
import { useStartImplementation } from '#renderer/domains/captain/runtime/hooks';
import type { TimelineEvent } from '#renderer/global/types';
import { PlanSummaryBlock } from '#renderer/domains/captain/ui/PlanCompletedBlock/PlanSummaryBlock';

export function ReadyPlanBlock({
  event,
  taskId,
}: {
  event: TimelineEvent;
  taskId: number;
}): React.ReactElement {
  const implMutation = useStartImplementation();

  return (
    <PlanSummaryBlock
      event={event}
      status="plan-ready"
      title="Plan ready for review"
      action={
        <button
          type="button"
          disabled={implMutation.isPending}
          onClick={() => implMutation.mutate({ id: taskId, message: 'Start implementation.' })}
          className="flex items-center gap-1.5 rounded-md bg-foreground px-3 py-1.5 text-body font-medium text-background hover:bg-foreground/90 disabled:opacity-50"
        >
          {implMutation.isPending ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <Play className="size-3.5" />
          )}
          Start implementation
        </button>
      }
    />
  );
}
