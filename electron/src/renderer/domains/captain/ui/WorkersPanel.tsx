import React, { useState } from 'react';
import {
  useResumeRateLimited,
  useTaskList,
  useWorkers,
} from '#renderer/domains/captain/runtime/hooks';
import {
  groupWorkersByPhase,
  workingTaskCount,
  deduplicatedActiveCount,
  findResumeTarget,
} from '#renderer/domains/captain/service/metricsHelpers';
import type { WorkerDetail } from '#renderer/global/types';
import { Button } from '#renderer/global/ui/primitives/button';
import { WorkerRow } from '#renderer/domains/captain/ui/WorkerRow';
import { MetricsHeaderContent } from '#renderer/domains/captain/ui/MetricsHeaderContent';

export function WorkersPanel({
  onNudge,
  onStopWorker,
}: {
  onNudge?: (worker: WorkerDetail) => void;
  onStopWorker?: (worker: WorkerDetail) => void | Promise<void>;
} = {}): React.ReactElement {
  const [expanded, setExpanded] = useState(true);
  const resumeMut = useResumeRateLimited();

  const { data: workersData } = useWorkers();

  const workers: WorkerDetail[] = workersData?.workers ?? [];
  const rateLimitSecs = workersData?.rate_limit_remaining_secs ?? 0;
  const grouped = groupWorkersByPhase(workers);
  const activeWorkers = grouped.active;
  const reviewingWorkers = grouped.reviewing;
  const mergingWorkers = grouped.merging;
  const staleWorkers = grouped.stale;
  const reviewingCount = reviewingWorkers.length;
  const mergingCount = mergingWorkers.length;
  const staleCount = staleWorkers.length;
  const { data: taskData } = useTaskList();
  const workingCount = workingTaskCount(taskData?.items ?? []);
  const activeCount = deduplicatedActiveCount(
    workingCount,
    reviewingCount,
    mergingCount,
    staleCount,
  );
  const resumeTaskId = findResumeTarget(grouped, taskData?.items ?? [], rateLimitSecs);
  const handleResume = resumeTaskId ? () => resumeMut.mutate({ id: resumeTaskId }) : undefined;

  const headerProps = {
    activeCount,
    reviewingCount,
    mergingCount,
    staleCount,
    rateLimitSecs,
    onResume: handleResume,
    resumePending: resumeMut.isPending,
  };

  return (
    <div data-testid="workers-panel" className="mb-1.5">
      {(!expanded || workers.length === 0) &&
        (workers.length > 0 ? (
          <Button
            variant="ghost"
            onClick={() => setExpanded(true)}
            aria-label="Expand workers panel"
            className="flex h-auto w-auto items-center gap-3 rounded-md px-4 py-1.5 text-text-3"
          >
            <MetricsHeaderContent {...headerProps} expanded={false} />
          </Button>
        ) : (
          <div className="flex items-center gap-3 rounded-md px-4 py-1.5 text-text-3">
            <MetricsHeaderContent
              {...headerProps}
              reviewingCount={0}
              mergingCount={0}
              staleCount={0}
              expanded={false}
            />
          </div>
        ))}

      {expanded && workers.length > 0 && (
        <div className="overflow-hidden rounded-md">
          <Button
            variant="ghost"
            onClick={() => setExpanded(false)}
            aria-label="Collapse workers panel"
            className="flex h-auto w-full items-center gap-3 rounded-none border-none bg-transparent px-4 py-2 text-text-3"
          >
            <MetricsHeaderContent {...headerProps} expanded={true} />
          </Button>

          <div className="mx-4 border-t border-border/40" />

          {activeWorkers.map((w) => (
            <WorkerRow
              key={w.id}
              worker={w}
              stale={false}
              onNudge={onNudge}
              onStop={onStopWorker}
            />
          ))}
          {reviewingWorkers.map((w) => (
            <WorkerRow key={w.id} worker={w} stale={false} />
          ))}
          {mergingWorkers.map((w) => (
            <WorkerRow key={w.id} worker={w} stale={false} />
          ))}
          {staleWorkers.map((w) => (
            <WorkerRow key={w.id} worker={w} stale={true} onNudge={onNudge} onStop={onStopWorker} />
          ))}

          <div className="h-1.5" />
        </div>
      )}
    </div>
  );
}
