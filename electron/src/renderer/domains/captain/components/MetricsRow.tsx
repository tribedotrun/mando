import React, { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { ChevronDown, MoreVertical } from 'lucide-react';
import { fetchWorkers } from '#renderer/domains/captain/hooks/useApi';
import { fmtRuntime, ceilMinutes, shortRepo } from '#renderer/utils';
import type { WorkerDetail } from '#renderer/types';
import { StatusDot } from '#renderer/global/components/CardShell';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '#renderer/components/ui/dropdown-menu';
import { Button } from '#renderer/components/ui/button';

type WorkerPhase = 'active' | 'reviewing' | 'merging' | 'stale';

function getWorkerPhase(worker: WorkerDetail, stale: boolean): WorkerPhase {
  if (worker.status === 'captain-reviewing') return 'reviewing';
  if (worker.status === 'captain-merging') return 'merging';
  return stale ? 'stale' : 'active';
}

const PHASE_COLORS: Record<
  WorkerPhase,
  { dot: string; text: string; duration: string; label?: string }
> = {
  active: {
    dot: 'var(--foreground)',
    text: 'var(--muted-foreground)',
    duration: 'var(--text-3)',
  },
  reviewing: {
    dot: 'var(--review)',
    text: 'var(--review)',
    duration: 'var(--review)',
    label: 'reviewing',
  },
  merging: {
    dot: 'var(--muted-foreground)',
    text: 'var(--muted-foreground)',
    duration: 'var(--muted-foreground)',
    label: 'merging',
  },
  stale: {
    dot: 'var(--stale)',
    text: 'var(--stale)',
    duration: 'var(--stale)',
    label: 'stale',
  },
};

function WorkerRow({
  worker,
  stale,
  onNudge,
  onStop,
}: {
  worker: WorkerDetail;
  stale: boolean;
  onNudge?: (worker: WorkerDetail) => void;
  onStop?: (worker: WorkerDetail) => void | Promise<void>;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [stopping, setStopping] = useState(false);
  const hasActions = !!onNudge || !!onStop;
  const phase = getWorkerPhase(worker, stale);
  const colors = PHASE_COLORS[phase];
  const dotColor = colors.dot;
  const textColor = colors.text;
  const durationColor = colors.duration;

  return (
    <div className="group relative flex min-h-[26px] items-center gap-2.5 px-4 py-1">
      {/* Status dot */}
      <StatusDot color={dotColor} size="sm" />

      {/* Task name */}
      <span className="min-w-0 flex-1 truncate text-[12px] leading-4" style={{ color: textColor }}>
        {worker.title}
      </span>

      {/* Project */}
      <span className="max-w-[80px] shrink-0 truncate text-[11px] leading-[14px] text-text-4">
        {shortRepo(worker.project)}
      </span>

      {/* Duration */}
      <span
        className="w-12 shrink-0 text-right text-[11px] leading-[14px]"
        style={{ color: durationColor }}
      >
        {fmtRuntime(
          phase === 'reviewing' || phase === 'merging'
            ? worker.last_activity_at
            : worker.started_at,
        )}
      </span>

      {/* Phase indicator */}
      {colors.label && (
        <span className="shrink-0 text-[11px] leading-[14px]" style={{ color: dotColor }}>
          {colors.label}
        </span>
      )}

      {hasActions && (
        <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon-xs"
              aria-label="Worker actions"
              className={`shrink-0 opacity-0 transition-opacity group-hover:opacity-100 ${menuOpen ? 'opacity-100' : ''}`}
            >
              <MoreVertical size={10} />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {onNudge && <DropdownMenuItem onSelect={() => onNudge(worker)}>Nudge</DropdownMenuItem>}
            {onStop && (
              <DropdownMenuItem
                variant="destructive"
                disabled={stopping}
                onSelect={(event) => {
                  event.preventDefault();
                  setStopping(true);
                  const result = onStop(worker);
                  if (result instanceof Promise) {
                    void result.finally(() => {
                      setStopping(false);
                      setMenuOpen(false);
                    });
                  } else {
                    setStopping(false);
                    setMenuOpen(false);
                  }
                }}
              >
                {stopping ? 'Stopping...' : 'Stop'}
              </DropdownMenuItem>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </div>
  );
}

export function MetricsRow({
  onNudge,
  onStopWorker,
}: {
  onNudge?: (worker: WorkerDetail) => void;
  onStopWorker?: (worker: WorkerDetail) => void | Promise<void>;
} = {}): React.ReactElement {
  const [expanded, setExpanded] = useState(true);

  const { data: workersData } = useQuery({
    queryKey: ['metrics-workers'],
    queryFn: fetchWorkers,
    refetchInterval: 15_000,
  });

  const workers: WorkerDetail[] = workersData?.workers ?? [];
  const rateLimitSecs = workersData?.rate_limit_remaining_secs ?? 0;
  const grouped = workers.reduce<Record<WorkerPhase, WorkerDetail[]>>(
    (acc, w) => {
      acc[getWorkerPhase(w, !!w.is_stale)].push(w);
      return acc;
    },
    { active: [], reviewing: [], merging: [], stale: [] },
  );
  const activeWorkers = grouped.active;
  const reviewingWorkers = grouped.reviewing;
  const mergingWorkers = grouped.merging;
  const staleWorkers = grouped.stale;
  const activeCount = activeWorkers.length;
  const reviewingCount = reviewingWorkers.length;
  const mergingCount = mergingWorkers.length;
  const staleCount = staleWorkers.length;

  return (
    <div data-testid="metrics-row" className="mb-3">
      {/* Collapsed pill, shown when collapsed or when no workers */}
      {(!expanded || workers.length === 0) &&
        (workers.length > 0 ? (
          <Button
            variant="ghost"
            onClick={() => setExpanded(true)}
            aria-label="Expand workers panel"
            className="flex h-auto w-auto items-center gap-3 rounded-md bg-card px-4 py-1 text-text-3"
          >
            <HeaderContent
              activeCount={activeCount}
              reviewingCount={reviewingCount}
              mergingCount={mergingCount}
              staleCount={staleCount}
              rateLimitSecs={rateLimitSecs}
              expanded={false}
            />
          </Button>
        ) : (
          <div className="flex items-center gap-3 rounded-md bg-card px-4 py-1 text-text-3">
            <HeaderContent
              activeCount={0}
              reviewingCount={0}
              mergingCount={0}
              staleCount={0}
              rateLimitSecs={rateLimitSecs}
              expanded={false}
            />
          </div>
        ))}

      {/* Expanded strip, only when there are workers to show */}
      {expanded && workers.length > 0 && (
        <div className="overflow-hidden rounded-md bg-card">
          {/* Strip header */}
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
            />
          </Button>

          {/* Active workers */}
          {activeWorkers.map((w) => (
            <WorkerRow
              key={w.id}
              worker={w}
              stale={false}
              onNudge={onNudge}
              onStop={onStopWorker}
            />
          ))}

          {/* Reviewing workers, no nudge/stop since captain is in control */}
          {reviewingWorkers.map((w) => (
            <WorkerRow key={w.id} worker={w} stale={false} />
          ))}

          {/* Merging workers, captain is merging, no user actions */}
          {mergingWorkers.map((w) => (
            <WorkerRow key={w.id} worker={w} stale={false} />
          ))}

          {/* Stale workers */}
          {staleWorkers.map((w) => (
            <WorkerRow key={w.id} worker={w} stale={true} onNudge={onNudge} onStop={onStopWorker} />
          ))}

          {/* Bottom padding */}
          <div className="h-1" />
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
}: {
  activeCount: number;
  reviewingCount: number;
  mergingCount: number;
  staleCount: number;
  rateLimitSecs: number;
  expanded: boolean;
}) {
  return (
    <>
      <span className="text-label text-text-3">Workers</span>
      <span className="text-[12px] leading-4 text-foreground">{activeCount} active</span>
      {reviewingCount > 0 && (
        <span className="text-[12px] leading-4 text-review">{reviewingCount} reviewing</span>
      )}
      {mergingCount > 0 && (
        <span className="text-[12px] leading-4 text-muted-foreground">{mergingCount} merging</span>
      )}
      {staleCount > 0 && (
        <span className="text-[12px] font-semibold leading-4 text-stale">{staleCount} stale</span>
      )}
      {rateLimitSecs > 0 && (
        <span className="text-[12px] leading-4 text-text-4">
          paused ~{ceilMinutes(rateLimitSecs)}m
        </span>
      )}
      <span className="flex-1" />
      {(activeCount > 0 || reviewingCount > 0 || mergingCount > 0 || staleCount > 0) && (
        <ChevronDown
          size={10}
          className={`transition-transform duration-150 ${expanded ? 'rotate-180' : ''}`}
        />
      )}
    </>
  );
}
