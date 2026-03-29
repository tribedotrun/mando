import React from 'react';
import { useQuery } from '@tanstack/react-query';
import { apiGet } from '#renderer/api';

interface TaskCounts {
  total: number;
  merged: number;
  completed_no_pr: number;
  errored: number;
  escalated: number;
  canceled: number;
  in_progress: number;
  queued: number;
  action_needed: number;
}

interface ProjectTasks {
  project: string;
  total: number;
  merged: number;
  errored: number;
}

interface AnalyticsData {
  task_counts: TaskCounts;
  project_tasks: ProjectTasks[];
}

export function AnalyticsPage(): React.ReactElement {
  const { data, isLoading, error } = useQuery<AnalyticsData>({
    queryKey: ['analytics'],
    queryFn: () => apiGet<AnalyticsData>('/api/analytics'),
    refetchInterval: 60_000,
  });

  if (isLoading) {
    return (
      <div className="flex items-center justify-center" style={{ height: 200 }}>
        <span className="text-body" style={{ color: 'var(--color-text-3)' }}>
          Loading analytics…
        </span>
      </div>
    );
  }

  if (error || !data) {
    return (
      <div className="flex items-center justify-center" style={{ height: 200 }}>
        <span className="text-body" style={{ color: 'var(--color-error)' }}>
          Failed to load analytics
        </span>
      </div>
    );
  }

  const tc = data.task_counts;
  const successCount = tc.merged + tc.completed_no_pr;
  const failedCount = tc.errored + tc.escalated;
  const attemptedCount = successCount + failedCount + tc.canceled;
  const successRate = attemptedCount > 0 ? Math.round((successCount / attemptedCount) * 100) : 0;

  const rateColor =
    successRate >= 70
      ? 'var(--color-success)'
      : successRate >= 40
        ? 'var(--color-stale)'
        : 'var(--color-error)';

  return (
    <div className="flex flex-col" style={{ gap: 20 }}>
      <h1 className="text-heading" style={{ color: 'var(--color-text-1)' }}>
        Analytics
      </h1>

      {/* Metric cards */}
      <div className="grid grid-cols-3" style={{ gap: 10 }}>
        <MetricCard
          label="Tasks completed"
          value={`${successCount}`}
          sub={`of ${tc.total} total`}
        />
        <MetricCard label="Success rate" value={`${successRate}%`} valueColor={rateColor} />
        <MetricCard
          label="Active"
          value={`${tc.in_progress + tc.queued}`}
          sub={`${tc.in_progress} working · ${tc.queued} queued`}
        />
      </div>

      {/* Task outcomes */}
      <TaskStatusBreakdown counts={tc} />

      {/* Tasks by project */}
      {data.project_tasks.length > 0 && <ProjectTable data={data.project_tasks} />}
    </div>
  );
}

/* ── Metric Card ────────────────────────────────────────────────────────── */

function MetricCard({
  label,
  value,
  sub,
  valueColor,
}: {
  label: string;
  value: string;
  sub?: string;
  valueColor?: string;
}): React.ReactElement {
  return (
    <div
      style={{
        background: 'var(--color-surface-1)',
        border: '1px solid var(--color-border-subtle)',
        borderRadius: 'var(--radius-panel)',
        padding: '14px 16px',
      }}
    >
      <div className="text-label" style={{ color: 'var(--color-text-3)', marginBottom: 6 }}>
        {label}
      </div>
      <div
        style={{
          fontSize: 28,
          fontWeight: 600,
          color: valueColor ?? 'var(--color-text-1)',
          letterSpacing: '-0.02em',
          lineHeight: 1,
        }}
      >
        {value}
      </div>
      {sub && (
        <div className="text-caption" style={{ color: 'var(--color-text-3)', marginTop: 5 }}>
          {sub}
        </div>
      )}
    </div>
  );
}

/* ── Task Status Breakdown ──────────────────────────────────────────────── */

const STATUS_COLORS: Record<string, string> = {
  merged: 'var(--color-success)',
  completed: 'var(--color-success)',
  errored: 'var(--color-error)',
  escalated: 'var(--color-needs-human)',
  canceled: 'var(--color-text-4)',
  in_progress: 'var(--color-accent)',
  queued: 'var(--color-stale)',
  action_needed: 'var(--color-review)',
};

function TaskStatusBreakdown({ counts }: { counts: TaskCounts }): React.ReactElement {
  const segments = [
    { key: 'merged', label: 'Merged', count: counts.merged, color: STATUS_COLORS.merged },
    {
      key: 'completed',
      label: 'Completed (no PR)',
      count: counts.completed_no_pr,
      color: STATUS_COLORS.completed,
    },
    {
      key: 'in_progress',
      label: 'In progress',
      count: counts.in_progress,
      color: STATUS_COLORS.in_progress,
    },
    { key: 'queued', label: 'Queued', count: counts.queued, color: STATUS_COLORS.queued },
    {
      key: 'action_needed',
      label: 'Action needed',
      count: counts.action_needed,
      color: STATUS_COLORS.action_needed,
    },
    { key: 'errored', label: 'Errored', count: counts.errored, color: STATUS_COLORS.errored },
    {
      key: 'escalated',
      label: 'Escalated',
      count: counts.escalated,
      color: STATUS_COLORS.escalated,
    },
    { key: 'canceled', label: 'Canceled', count: counts.canceled, color: STATUS_COLORS.canceled },
  ].filter((s) => s.count > 0);

  const total = counts.total || 1;

  return (
    <div
      style={{
        background: 'var(--color-surface-1)',
        border: '1px solid var(--color-border-subtle)',
        borderRadius: 'var(--radius-panel)',
        padding: '16px 16px 12px',
      }}
    >
      <div className="text-label" style={{ color: 'var(--color-text-3)', marginBottom: 12 }}>
        Task outcomes
      </div>

      {/* Stacked bar — continuous, no gaps */}
      <div
        className="flex overflow-hidden"
        style={{ height: 32, borderRadius: 5, marginBottom: 12 }}
      >
        {segments.map((s) => (
          <div
            key={s.key}
            style={{
              width: `${(s.count / total) * 100}%`,
              background: s.color,
              opacity: 0.8,
              minWidth: s.count > 0 ? 3 : 0,
            }}
            title={`${s.label}: ${s.count}`}
          />
        ))}
        {segments.length === 0 && <div style={{ flex: 1, background: 'var(--color-surface-3)' }} />}
      </div>

      {/* Legend */}
      <div className="flex flex-wrap" style={{ gap: '4px 14px' }}>
        {segments.map((s) => (
          <div key={s.key} className="flex items-center" style={{ gap: 5 }}>
            <span
              style={{
                width: 7,
                height: 7,
                borderRadius: 2,
                background: s.color,
                opacity: 0.8,
                flexShrink: 0,
              }}
            />
            <span className="text-caption" style={{ color: 'var(--color-text-2)' }}>
              {s.label}
            </span>
            <span className="text-caption" style={{ color: 'var(--color-text-3)' }}>
              {s.count}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

/* ── Project Table ──────────────────────────────────────────────────────── */

function ProjectTable({ data }: { data: ProjectTasks[] }): React.ReactElement {
  return (
    <div
      style={{
        background: 'var(--color-surface-1)',
        border: '1px solid var(--color-border-subtle)',
        borderRadius: 'var(--radius-panel)',
        padding: '16px 16px 14px',
      }}
    >
      <div className="text-label" style={{ color: 'var(--color-text-3)', marginBottom: 10 }}>
        Tasks by project
      </div>
      <div className="flex flex-col" style={{ gap: 2 }}>
        {data.slice(0, 8).map((p) => {
          const successPct = p.total > 0 ? Math.round((p.merged / p.total) * 100) : 0;
          const taskLabel = p.total === 1 ? '1 task' : `${p.total} tasks`;
          const pctColor =
            successPct >= 70
              ? 'var(--color-success)'
              : successPct >= 40
                ? 'var(--color-stale)'
                : 'var(--color-text-3)';
          return (
            <div
              key={p.project}
              className="flex items-center text-body"
              style={{ padding: '5px 0' }}
            >
              <span className="truncate" style={{ flex: 1, color: 'var(--color-text-1)' }}>
                {p.project}
              </span>
              <span className="text-caption" style={{ color: 'var(--color-text-3)' }}>
                {taskLabel}
              </span>
              <span
                className="text-caption"
                style={{ width: 40, textAlign: 'right', color: pctColor }}
              >
                {successPct}%
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
