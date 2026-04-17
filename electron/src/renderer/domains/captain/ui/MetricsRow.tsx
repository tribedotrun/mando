import React, { useState } from 'react';
import { ChevronDown } from 'lucide-react';
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
import { ceilMinutes } from '#renderer/global/service/utils';
import type { WorkerDetail } from '#renderer/global/types';
import { Button } from '#renderer/global/ui/button';
import { WorkerRow } from '#renderer/domains/captain/ui/WorkerRow';

export function MetricsRow({
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

  return (
    <div data-testid="metrics-row" className="mb-1.5">
      {(!expanded || workers.length === 0) &&
        (workers.length > 0 ? (
          <Button
            variant="ghost"
            onClick={() => setExpanded(true)}
            aria-label="Expand workers panel"
            className="flex h-auto w-auto items-center gap-3 rounded-md px-4 py-1.5 text-text-3"
          >
            <HeaderContent
              activeCount={activeCount}
              reviewingCount={reviewingCount}
              mergingCount={mergingCount}
              staleCount={staleCount}
              rateLimitSecs={rateLimitSecs}
              expanded={false}
              onResume={handleResume}
              resumePending={resumeMut.isPending}
            />
          </Button>
        ) : (
          <div className="flex items-center gap-3 rounded-md px-4 py-1.5 text-text-3">
            <HeaderContent
              activeCount={activeCount}
              reviewingCount={0}
              mergingCount={0}
              staleCount={0}
              rateLimitSecs={rateLimitSecs}
              expanded={false}
              onResume={handleResume}
              resumePending={resumeMut.isPending}
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
            <HeaderContent
              activeCount={activeCount}
              reviewingCount={reviewingCount}
              mergingCount={mergingCount}
              staleCount={staleCount}
              rateLimitSecs={rateLimitSecs}
              expanded={true}
              onResume={handleResume}
              resumePending={resumeMut.isPending}
            />
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

function HeaderContent({
  activeCount,
  reviewingCount,
  mergingCount,
  staleCount,
  rateLimitSecs,
  expanded,
  onResume,
  resumePending,
}: {
  activeCount: number;
  reviewingCount: number;
  mergingCount: number;
  staleCount: number;
  rateLimitSecs: number;
  expanded: boolean;
  onResume?: () => void;
  resumePending?: boolean;
}) {
  return (
    <>
      <span className="text-label text-text-3">Workers</span>
      <span className={`text-[12px] leading-4 ${activeCount > 0 ? 'text-success' : 'text-text-4'}`}>
        {activeCount} active
      </span>
      {reviewingCount > 0 && (
        <span className="text-[12px] leading-4 text-review">{reviewingCount} reviewing</span>
      )}
      {mergingCount > 0 && (
        <span className="text-[12px] leading-4 text-success">{mergingCount} merging</span>
      )}
      {staleCount > 0 && (
        <span className="text-[12px] leading-4 text-stale">{staleCount} stale</span>
      )}
      {rateLimitSecs > 0 && (
        <span className="inline-flex items-center gap-1.5 text-[12px] leading-4 text-text-4">
          paused ~{ceilMinutes(rateLimitSecs)}m
          {onResume && (
            <button
              type="button"
              disabled={resumePending}
              className="rounded px-1 py-0.5 text-[11px] font-medium text-foreground hover:bg-accent disabled:opacity-50"
              onClick={(e) => {
                e.stopPropagation();
                onResume();
              }}
            >
              {resumePending ? 'Resuming...' : 'Resume'}
            </button>
          )}
        </span>
      )}
      <span className="flex-1" />
      {(activeCount > 0 || reviewingCount > 0 || mergingCount > 0 || staleCount > 0) && (
        <ChevronDown
          size={10}
          className={`transition-transform duration-150 ease-out ${expanded ? 'rotate-180' : ''}`}
        />
      )}
    </>
  );
}
