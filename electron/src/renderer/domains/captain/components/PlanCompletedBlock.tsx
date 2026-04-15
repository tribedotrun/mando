import React, { useState } from 'react';
import { ChevronDown, ChevronRight, Play, Loader2 } from 'lucide-react';
import { useStartImplementation } from '#renderer/hooks/mutations';
import { StatusIcon } from '#renderer/global/components/StatusIndicator';
import type { TimelineEvent } from '#renderer/types';

export function PlanCompletedBlock({
  event,
  isPlanReady,
  taskId,
  taskContext,
}: {
  event: TimelineEvent;
  isPlanReady: boolean;
  taskId: number;
  taskContext: string;
}) {
  const [planOpen, setPlanOpen] = useState(false);
  const implMutation = useStartImplementation();
  const diagram = (event.data?.diagram as string) || '';
  const plan = (event.data?.plan as string) || '';
  const time = new Date(event.timestamp).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });
  return (
    <div className="mx-3 my-2 space-y-3 rounded-lg bg-muted/40 px-4 py-3">
      <div className="flex items-center gap-2">
        <StatusIcon status={isPlanReady ? 'plan-ready' : 'completed-no-pr'} />
        <span className="text-body font-medium text-text-1">
          {isPlanReady ? 'Plan ready for review' : 'Planning complete'}
        </span>
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
      {isPlanReady && (
        <button
          type="button"
          disabled={implMutation.isPending}
          onClick={() => implMutation.mutate({ id: taskId, context: taskContext })}
          className="flex items-center gap-1.5 rounded-md bg-foreground px-3 py-1.5 text-body font-medium text-background hover:bg-foreground/90 disabled:opacity-50"
        >
          {implMutation.isPending ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <Play className="size-3.5" />
          )}
          Start implementation
        </button>
      )}
    </div>
  );
}
