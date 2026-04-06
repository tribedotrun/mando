import React from 'react';

const STATUSES = ['all', 'pending', 'fetched', 'processed', 'saved', 'archived', 'error'];

interface Props {
  activeStatus: string;
  onStatusChange: (status: string) => void;
  statusCounts: Record<string, number>;
}

export function ScoutStatusTabs({
  activeStatus,
  onStatusChange,
  statusCounts,
}: Props): React.ReactElement {
  const allCount = Object.values(statusCounts).reduce((a, b) => a + b, 0);

  return (
    <div
      data-testid="scout-status-tabs"
      className="flex items-center"
      style={{ borderBottom: '1px solid var(--color-border-subtle)', gap: 0 }}
    >
      {STATUSES.map((s) => {
        const count = s === 'all' ? allCount : (statusCounts[s] ?? 0);
        const active = activeStatus === s;
        const isError = s === 'error' && count > 0;
        return (
          <button
            key={s}
            onClick={() => onStatusChange(s)}
            className="text-[13px] transition-colors"
            style={{
              background: 'transparent',
              color: isError
                ? 'var(--color-error)'
                : active
                  ? 'var(--color-text-1)'
                  : 'var(--color-text-2)',
              fontWeight: active ? 500 : 400,
              padding: '6px 12px',
              border: 'none',
              borderBottomWidth: 2,
              borderBottomStyle: 'solid',
              borderBottomColor: active ? 'var(--color-accent)' : 'transparent',
              cursor: 'pointer',
              marginBottom: -1,
            }}
          >
            {s}
            {count > 0 && (
              <span
                className="ml-1"
                style={{
                  fontSize: 12,
                  color: active ? 'var(--color-text-2)' : 'var(--color-text-3)',
                }}
              >
                {count}
              </span>
            )}
          </button>
        );
      })}
    </div>
  );
}
