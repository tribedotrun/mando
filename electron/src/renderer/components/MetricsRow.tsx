import React, { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { fetchWorkers } from '#renderer/api';
import type { WorkerDetail } from '#renderer/types';

function formatRuntime(startedAt?: string): string {
  if (!startedAt) return '-';
  const start = new Date(startedAt).getTime();
  if (Number.isNaN(start)) return '-';
  const diffMs = Date.now() - start;
  if (diffMs < 0) return '-';
  const totalMin = Math.floor(diffMs / 60_000);
  const h = Math.floor(totalMin / 60);
  const m = totalMin % 60;
  return h > 0 ? `${h}h ${m}m` : `${m}m`;
}

function WorkerRow({ worker, stale }: { worker: WorkerDetail; stale: boolean }) {
  const dotColor = stale ? 'var(--color-stale)' : 'var(--color-success)';
  const textColor = stale ? 'var(--color-stale)' : 'var(--color-text-2)';
  const durationColor = stale ? 'var(--color-stale)' : 'var(--color-text-3)';

  return (
    <div className="flex items-center" style={{ padding: '5px 14px', gap: 10, minHeight: 26 }}>
      {/* Status dot */}
      <span
        aria-hidden="true"
        style={{
          width: 4,
          height: 4,
          borderRadius: '50%',
          background: dotColor,
          flexShrink: 0,
        }}
      />

      {/* Task name */}
      <span
        className="min-w-0 flex-1 truncate"
        style={{ fontSize: 12, color: textColor, lineHeight: '16px' }}
        title={worker.title}
      >
        {worker.title}
      </span>

      {/* Project */}
      <span
        className="shrink-0 truncate"
        style={{
          fontSize: 11,
          color: 'var(--color-text-4)',
          lineHeight: '14px',
          maxWidth: 80,
        }}
      >
        {worker.project?.split('/').pop()}
      </span>

      {/* Duration */}
      <span
        className="shrink-0 text-right"
        style={{
          fontSize: 11,
          color: durationColor,
          lineHeight: '14px',
          width: 48,
        }}
      >
        {formatRuntime(worker.started_at)}
      </span>

      {/* Stale indicator */}
      {stale && (
        <span
          style={{
            fontSize: 10,
            color: 'var(--color-stale)',
            lineHeight: '14px',
            flexShrink: 0,
          }}
        >
          stale
        </span>
      )}
    </div>
  );
}

export function MetricsRow(): React.ReactElement {
  const [expanded, setExpanded] = useState(true);

  const { data: workersData } = useQuery({
    queryKey: ['metrics-workers'],
    queryFn: fetchWorkers,
    refetchInterval: 15_000,
  });

  const workers: WorkerDetail[] = workersData?.workers ?? [];
  const activeWorkers = workers.filter((w) => !w.is_stale);
  const staleWorkers = workers.filter((w) => w.is_stale);
  const activeCount = activeWorkers.length;
  const staleCount = staleWorkers.length;

  return (
    <div data-testid="metrics-row" style={{ marginBottom: 12 }}>
      {/* Collapsed pill — shown when collapsed or when no workers */}
      {(!expanded || workers.length === 0) &&
        (workers.length > 0 ? (
          <button
            onClick={() => setExpanded(true)}
            aria-label="Expand workers panel"
            className="flex items-center"
            style={{
              background: 'var(--color-surface-1)',
              border: '1px solid var(--color-border-subtle)',
              borderRadius: 6,
              padding: '6px 14px',
              gap: 12,
              cursor: 'pointer',
              color: 'var(--color-text-3)',
            }}
          >
            <HeaderContent activeCount={activeCount} staleCount={staleCount} expanded={false} />
          </button>
        ) : (
          <div
            className="flex items-center"
            style={{
              background: 'var(--color-surface-1)',
              border: '1px solid var(--color-border-subtle)',
              borderRadius: 6,
              padding: '6px 14px',
              gap: 12,
              color: 'var(--color-text-3)',
            }}
          >
            <HeaderContent activeCount={0} staleCount={0} expanded={false} />
          </div>
        ))}

      {/* Expanded strip — only when there are workers to show */}
      {expanded && workers.length > 0 && (
        <div
          style={{
            background: 'var(--color-surface-1)',
            borderRadius: 6,
            overflow: 'hidden',
          }}
        >
          {/* Strip header */}
          <button
            onClick={() => setExpanded(false)}
            aria-label="Collapse workers panel"
            className="flex w-full items-center"
            style={{
              padding: '9px 14px',
              gap: 12,
              cursor: 'pointer',
              background: 'none',
              border: 'none',
              color: 'var(--color-text-3)',
            }}
          >
            <HeaderContent activeCount={activeCount} staleCount={staleCount} expanded={true} />
          </button>

          {/* Active workers */}
          {activeWorkers.map((w) => (
            <WorkerRow key={w.id} worker={w} stale={false} />
          ))}

          {/* Stale workers */}
          {staleWorkers.map((w) => (
            <WorkerRow key={w.id} worker={w} stale={true} />
          ))}

          {/* Bottom padding */}
          <div style={{ height: 4 }} />
        </div>
      )}
    </div>
  );
}

function HeaderContent({
  activeCount,
  staleCount,
  expanded,
}: {
  activeCount: number;
  staleCount: number;
  expanded: boolean;
}) {
  return (
    <>
      <span
        style={{
          fontSize: 10,
          fontWeight: 600,
          color: 'var(--color-text-3)',
          textTransform: 'uppercase' as const,
          letterSpacing: '0.06em',
          lineHeight: '14px',
        }}
      >
        Workers
      </span>
      <span style={{ fontSize: 12, color: 'var(--color-success)', lineHeight: '16px' }}>
        {activeCount} active
      </span>
      {staleCount > 0 && (
        <span
          style={{
            fontSize: 12,
            fontWeight: 600,
            color: 'var(--color-stale)',
            lineHeight: '16px',
          }}
        >
          {staleCount} stale
        </span>
      )}
      <span style={{ flex: 1 }} />
      {(activeCount > 0 || staleCount > 0) && (
        <svg
          width="10"
          height="10"
          viewBox="0 0 10 10"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
          style={{
            transform: expanded ? 'rotate(180deg)' : 'rotate(0)',
            transition: 'transform 150ms',
          }}
        >
          <path d="M2 4l3 3 3-3" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      )}
    </>
  );
}
