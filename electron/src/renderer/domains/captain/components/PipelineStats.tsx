import React, { useMemo } from 'react';
import { useTaskList } from '#renderer/hooks/queries';
import {
  ACTION_NEEDED_STATUSES,
  FINALIZED_STATUSES,
  type TaskItem,
  type ItemStatus,
} from '#renderer/types';

interface StatCounts {
  queued: number;
  inProgress: number;
  reviewing: number;
  merging: number;
  actionNeeded: number;
  errored: number;
  mergedToday: number;
  total: number;
}

function computeCounts(items: TaskItem[]): StatCounts {
  const now = new Date();
  const todayStart = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const actionSet = new Set<ItemStatus>(ACTION_NEEDED_STATUSES);
  const finalSet = new Set<ItemStatus>(FINALIZED_STATUSES);

  let queued = 0;
  let inProgress = 0;
  let reviewing = 0;
  let merging = 0;
  let actionNeeded = 0;
  let errored = 0;
  let mergedToday = 0;
  let total = 0;

  for (const t of items) {
    if (finalSet.has(t.status) && t.status !== 'merged') continue;
    if (t.status === 'merged') {
      const ts = t.last_activity_at ? new Date(t.last_activity_at).getTime() : 0;
      if (ts >= todayStart) mergedToday++;
      continue;
    }

    total++;
    switch (t.status) {
      case 'new':
      case 'queued':
        queued++;
        break;
      case 'in-progress':
      case 'clarifying':
      case 'rework':
      case 'handed-off':
        inProgress++;
        break;
      case 'captain-reviewing':
        reviewing++;
        break;
      case 'captain-merging':
        merging++;
        break;
      case 'errored':
        errored++;
        break;
      default:
        if (actionSet.has(t.status)) actionNeeded++;
        break;
    }
  }

  return { queued, inProgress, reviewing, merging, actionNeeded, errored, mergedToday, total };
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
  const { data } = useTaskList();
  const counts = useMemo(() => computeCounts(data?.items ?? []), [data?.items]);

  // Only show stats that have nonzero values, except always show "in pipeline"
  const stats: { value: number; label: string; color: string }[] = [
    { value: counts.total, label: 'In pipeline', color: 'var(--foreground)' },
  ];
  if (counts.queued > 0)
    stats.push({ value: counts.queued, label: 'Queued', color: 'var(--muted-foreground)' });
  if (counts.inProgress > 0)
    stats.push({ value: counts.inProgress, label: 'Working', color: 'var(--success)' });
  if (counts.reviewing > 0)
    stats.push({ value: counts.reviewing, label: 'Reviewing', color: 'var(--review)' });
  if (counts.merging > 0)
    stats.push({ value: counts.merging, label: 'Merging', color: 'var(--success)' });
  if (counts.actionNeeded > 0)
    stats.push({ value: counts.actionNeeded, label: 'Action needed', color: 'var(--needs-human)' });
  if (counts.errored > 0)
    stats.push({ value: counts.errored, label: 'Errored', color: 'var(--destructive)' });
  if (counts.mergedToday > 0)
    stats.push({ value: counts.mergedToday, label: 'Merged today', color: 'var(--text-3)' });

  return (
    <div data-testid="pipeline-stats" className="flex items-center justify-center gap-8 py-2">
      {stats.map((s) => (
        <Stat key={s.label} value={s.value} label={s.label} color={s.color} />
      ))}
    </div>
  );
}
