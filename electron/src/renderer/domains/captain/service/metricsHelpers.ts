import { WORKING_STATUSES, type TaskItem, type WorkerDetail } from '#renderer/global/types';
import { isRateLimited } from '#renderer/global/service/utils';

export type WorkerPhase = 'active' | 'reviewing' | 'merging' | 'stale';

export function getWorkerPhase(worker: WorkerDetail, stale: boolean): WorkerPhase {
  if (worker.status === 'captain-reviewing') return 'reviewing';
  if (worker.status === 'captain-merging') return 'merging';
  return stale ? 'stale' : 'active';
}

export function groupWorkersByPhase(workers: WorkerDetail[]): Record<WorkerPhase, WorkerDetail[]> {
  const grouped: Record<WorkerPhase, WorkerDetail[]> = {
    active: [],
    reviewing: [],
    merging: [],
    stale: [],
  };
  for (const w of workers) grouped[getWorkerPhase(w, !!w.is_stale)].push(w);
  return grouped;
}

/** Count tasks in working statuses. */
export function workingTaskCount(tasks: TaskItem[]): number {
  const workingSet = new Set<string>(WORKING_STATUSES);
  return tasks.filter((t) => workingSet.has(t.status)).length;
}

export function deduplicatedActiveCount(
  working: number,
  reviewing: number,
  merging: number,
  stale: number,
): number {
  const raw = working - reviewing - merging - stale;
  return raw > 0 ? raw : 0;
}

/** Find the task ID to resume (first reviewing/merging worker, or first rate-limited task). */
export function findResumeTarget(
  grouped: Record<WorkerPhase, WorkerDetail[]>,
  tasks: TaskItem[],
  rateLimitSecs: number,
): number | undefined {
  const workerResumeId = (grouped.reviewing[0] ?? grouped.merging[0])?.id;
  if (workerResumeId) return workerResumeId;
  if (rateLimitSecs > 0) return tasks.find((t) => isRateLimited(t, rateLimitSecs))?.id;
  return undefined;
}

export const PHASE_COLORS: Record<
  WorkerPhase,
  { dot: string; text: string; duration: string; label?: string }
> = {
  active: {
    dot: 'var(--success)',
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
    dot: 'var(--success)',
    text: 'var(--success)',
    duration: 'var(--success)',
    label: 'merging',
  },
  stale: {
    dot: 'var(--stale)',
    text: 'var(--stale)',
    duration: 'var(--stale)',
    label: 'stale',
  },
};
