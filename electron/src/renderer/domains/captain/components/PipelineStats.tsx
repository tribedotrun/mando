import React, { useMemo } from 'react';
import { useTaskList, useActivityStats } from '#renderer/hooks/queries';
import {
  ACTION_NEEDED_STATUSES,
  FINALIZED_STATUSES,
  type TaskItem,
  type ItemStatus,
} from '#renderer/types';

interface StatCounts {
  queued: number;
  working: number;
  actionNeeded: number;
  errored: number;
}

function computeCounts(items: TaskItem[]): StatCounts {
  const actionSet = new Set<ItemStatus>(ACTION_NEEDED_STATUSES);
  const finalSet = new Set<ItemStatus>(FINALIZED_STATUSES);

  let queued = 0;
  let working = 0;
  let actionNeeded = 0;
  let errored = 0;

  for (const t of items) {
    if (finalSet.has(t.status)) continue;

    switch (t.status) {
      case 'new':
      case 'queued':
        queued++;
        break;
      case 'in-progress':
      case 'clarifying':
      case 'rework':
      case 'handed-off':
      case 'captain-reviewing':
      case 'captain-merging':
        working++;
        break;
      case 'errored':
        errored++;
        break;
      default:
        if (actionSet.has(t.status)) actionNeeded++;
        break;
    }
  }

  return { queued, working, actionNeeded, errored };
}

function Stat({ value, label, color }: { value: number; label: string; color: string }) {
  return (
    <div className="flex flex-col items-center gap-1">
      <span className="text-[20px] font-semibold leading-6" style={{ color }}>
        {value}
      </span>
      <span className="text-label text-text-4">{label}</span>
    </div>
  );
}

export function PipelineStats(): React.ReactElement {
  const { data: taskData } = useTaskList();
  const { data: statsData } = useActivityStats();
  const counts = useMemo(() => computeCounts(taskData?.items ?? []), [taskData?.items]);
  const merged7d = statsData?.merged_7d ?? 0;

  const stats: { value: number; label: string; color: string }[] = [];

  if (counts.actionNeeded > 0)
    stats.push({
      value: counts.actionNeeded,
      label: 'Action needed',
      color: 'var(--needs-human)',
    });
  if (counts.working > 0)
    stats.push({ value: counts.working, label: 'Working', color: 'var(--success)' });
  if (counts.errored > 0)
    stats.push({ value: counts.errored, label: 'Errored', color: 'var(--destructive)' });
  if (counts.queued > 0)
    stats.push({ value: counts.queued, label: 'Queued', color: 'var(--muted-foreground)' });
  if (merged7d > 0) stats.push({ value: merged7d, label: 'Merged (7d)', color: 'var(--text-3)' });

  if (stats.length === 0) return <div data-testid="pipeline-stats" />;

  return (
    <div data-testid="pipeline-stats" className="flex items-center justify-center gap-8 py-2">
      {stats.map((s) => (
        <Stat key={s.label} value={s.value} label={s.label} color={s.color} />
      ))}
    </div>
  );
}
