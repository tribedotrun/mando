import React, { useState, useRef } from 'react';
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

type WorkerPhase = 'active' | 'reviewing' | 'merging' | 'stale';

function getWorkerPhase(worker: WorkerDetail, stale: boolean): WorkerPhase {
  if (worker.status === 'captain-reviewing') return 'reviewing';
  if (worker.status === 'captain-merging') return 'merging';
  return stale ? 'stale' : 'active';
}

const PHASE_COLORS: Record<WorkerPhase, { dot: string; text: string; duration: string }> = {
  active: {
    dot: 'var(--color-success)',
    text: 'var(--color-text-2)',
    duration: 'var(--color-text-3)',
  },
  reviewing: {
    dot: 'var(--color-accent)',
    text: 'var(--color-accent)',
    duration: 'var(--color-accent)',
  },
  merging: {
    dot: 'var(--color-text-2)',
    text: 'var(--color-text-2)',
    duration: 'var(--color-text-2)',
  },
  stale: {
    dot: 'var(--color-stale)',
    text: 'var(--color-stale)',
    duration: 'var(--color-stale)',
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
  onStop?: (worker: WorkerDetail) => void;
}) {
  const [menuOpenRaw, setMenuOpen] = useState(false);
  const btnRef = useRef<HTMLButtonElement>(null);
  const hasActions = !!onNudge || !!onStop;
  const menuOpen = hasActions && menuOpenRaw;
  const phase = getWorkerPhase(worker, stale);
  const colors = PHASE_COLORS[phase];
  const dotColor = colors.dot;
  const textColor = colors.text;
  const durationColor = colors.duration;

  return (
    <div
      className="group relative flex items-center"
      style={{ padding: '5px 14px', gap: 10, minHeight: 26 }}
    >
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

      {/* Phase indicator */}
      {phase === 'stale' && (
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
      {phase === 'reviewing' && (
        <span
          style={{
            fontSize: 10,
            color: 'var(--color-accent)',
            lineHeight: '14px',
            flexShrink: 0,
          }}
        >
          reviewing
        </span>
      )}
      {phase === 'merging' && (
        <span
          style={{
            fontSize: 10,
            color: 'var(--color-text-2)',
            lineHeight: '14px',
            flexShrink: 0,
          }}
        >
          merging
        </span>
      )}

      {hasActions && (
        <>
          {/* Overflow menu trigger */}
          <button
            ref={btnRef}
            onClick={() => setMenuOpen((v) => !v)}
            aria-label="Worker actions"
            className="shrink-0 rounded opacity-0 transition-opacity group-hover:opacity-100"
            style={{
              width: 20,
              height: 20,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              background: 'transparent',
              border: 'none',
              color: 'var(--color-text-2)',
              cursor: 'pointer',
              ...(menuOpen ? { opacity: 1 } : {}),
            }}
          >
            <svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor">
              <circle cx="8" cy="3" r="1.5" />
              <circle cx="8" cy="8" r="1.5" />
              <circle cx="8" cy="13" r="1.5" />
            </svg>
          </button>

          {/* Dropdown menu — fixed positioning to escape overflow:hidden parent */}
          {menuOpen && (
            <>
              <div className="fixed inset-0 z-40" onClick={() => setMenuOpen(false)} />
              <div
                className="fixed z-50 rounded border py-1"
                style={{
                  top: btnRef.current ? btnRef.current.getBoundingClientRect().bottom + 4 : 0,
                  left: btnRef.current ? btnRef.current.getBoundingClientRect().right - 100 : 0,
                  background: 'var(--color-surface-2)',
                  borderColor: 'var(--color-border)',
                  boxShadow: '0 4px 12px rgba(0,0,0,0.3)',
                  minWidth: 100,
                }}
              >
                {onNudge && (
                  <button
                    onClick={() => {
                      setMenuOpen(false);
                      onNudge(worker);
                    }}
                    className="flex w-full items-center px-3 py-1.5 text-left text-xs"
                    style={{
                      color: 'var(--color-text-2)',
                      background: 'none',
                      border: 'none',
                      cursor: 'pointer',
                    }}
                  >
                    Nudge
                  </button>
                )}
                {onStop && (
                  <button
                    onClick={() => {
                      setMenuOpen(false);
                      onStop(worker);
                    }}
                    className="flex w-full items-center px-3 py-1.5 text-left text-xs"
                    style={{
                      color: 'var(--color-error)',
                      background: 'none',
                      border: 'none',
                      cursor: 'pointer',
                    }}
                  >
                    Stop
                  </button>
                )}
              </div>
            </>
          )}
        </>
      )}
    </div>
  );
}

export function MetricsRow({
  onNudge,
  onStopWorker,
}: {
  onNudge?: (worker: WorkerDetail) => void;
  onStopWorker?: (worker: WorkerDetail) => void;
} = {}): React.ReactElement {
  const [expanded, setExpanded] = useState(true);

  const { data: workersData } = useQuery({
    queryKey: ['metrics-workers'],
    queryFn: fetchWorkers,
    refetchInterval: 15_000,
  });

  const workers: WorkerDetail[] = workersData?.workers ?? [];
  const rateLimitSecs = workersData?.rate_limit_remaining_secs ?? 0;
  const activeWorkers = workers.filter((w) => getWorkerPhase(w, !!w.is_stale) === 'active');
  const reviewingWorkers = workers.filter((w) => getWorkerPhase(w, !!w.is_stale) === 'reviewing');
  const mergingWorkers = workers.filter((w) => getWorkerPhase(w, !!w.is_stale) === 'merging');
  const staleWorkers = workers.filter((w) => getWorkerPhase(w, !!w.is_stale) === 'stale');
  const activeCount = activeWorkers.length;
  const reviewingCount = reviewingWorkers.length;
  const mergingCount = mergingWorkers.length;
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
            <HeaderContent
              activeCount={activeCount}
              reviewingCount={reviewingCount}
              mergingCount={mergingCount}
              staleCount={staleCount}
              rateLimitSecs={rateLimitSecs}
              expanded={false}
            />
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
            <HeaderContent
              activeCount={activeCount}
              reviewingCount={reviewingCount}
              mergingCount={mergingCount}
              staleCount={staleCount}
              rateLimitSecs={rateLimitSecs}
              expanded={true}
            />
          </button>

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

          {/* Reviewing workers — no nudge/stop since captain is in control */}
          {reviewingWorkers.map((w) => (
            <WorkerRow key={w.id} worker={w} stale={false} />
          ))}

          {/* Merging workers — captain is merging, no user actions */}
          {mergingWorkers.map((w) => (
            <WorkerRow key={w.id} worker={w} stale={false} />
          ))}

          {/* Stale workers */}
          {staleWorkers.map((w) => (
            <WorkerRow key={w.id} worker={w} stale={true} onNudge={onNudge} onStop={onStopWorker} />
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
      <span
        style={{
          fontSize: 10,
          fontWeight: 600,
          color: 'var(--color-text-3)',
          textTransform: 'uppercase',
          letterSpacing: '0.06em',
          lineHeight: '14px',
        }}
      >
        Workers
      </span>
      <span style={{ fontSize: 12, color: 'var(--color-success)', lineHeight: '16px' }}>
        {activeCount} active
      </span>
      {reviewingCount > 0 && (
        <span style={{ fontSize: 12, color: 'var(--color-accent)', lineHeight: '16px' }}>
          {reviewingCount} reviewing
        </span>
      )}
      {mergingCount > 0 && (
        <span style={{ fontSize: 12, color: 'var(--color-text-2)', lineHeight: '16px' }}>
          {mergingCount} merging
        </span>
      )}
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
      {rateLimitSecs > 0 && (
        <span style={{ fontSize: 12, color: 'var(--color-text-4)', lineHeight: '16px' }}>
          paused ~{Math.ceil(rateLimitSecs / 60)}m
        </span>
      )}
      <span style={{ flex: 1 }} />
      {(activeCount > 0 || reviewingCount > 0 || mergingCount > 0 || staleCount > 0) && (
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
