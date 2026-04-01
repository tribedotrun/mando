import React from 'react';
import type { ItemStatus } from '#renderer/types';
import { ACTION_NEEDED_STATUSES, IN_PROGRESS_STATUSES } from '#renderer/types';
import { useTaskStore } from '#renderer/stores/taskStore';
import { ViewOptions } from '#renderer/components/ViewOptions';

type FilterKey = ItemStatus | 'action-needed' | 'in-progress-group' | null;

interface FilterTab {
  key: FilterKey;
  label: string;
}

const TABS: FilterTab[] = [
  { key: null, label: 'All' },
  { key: 'action-needed', label: 'Action needed' },
  { key: 'in-progress-group', label: 'In progress' },
  { key: 'errored', label: 'Errored' },
  { key: 'queued', label: 'Queued' },
];

interface Props {
  projectFilter?: string | null;
}

export function StatusFilter({ projectFilter }: Props): React.ReactElement {
  const statusFilter = useTaskStore((s) => s.statusFilter);
  const setFilter = useTaskStore((s) => s.setFilter);
  const items = useTaskStore((s) => s.items);
  const showArchived = useTaskStore((s) => s.showArchived);

  const filtered = React.useMemo(
    () => (projectFilter ? items.filter((i) => i.project === projectFilter) : items),
    [items, projectFilter],
  );

  const counts = React.useMemo(() => {
    const c: Record<string, number> = {};
    const visible = showArchived ? filtered : filtered.filter((i) => !i.archived_at);
    for (const item of visible) {
      c[item.status] = (c[item.status] || 0) + 1;
      if (ACTION_NEEDED_STATUSES.includes(item.status)) {
        c['action-needed'] = (c['action-needed'] || 0) + 1;
      }
      if (IN_PROGRESS_STATUSES.includes(item.status)) {
        c['in-progress-group'] = (c['in-progress-group'] || 0) + 1;
      }
    }
    return c;
  }, [filtered, showArchived]);

  const totalCount = React.useMemo(
    () => (showArchived ? filtered.length : filtered.filter((i) => !i.archived_at).length),
    [filtered, showArchived],
  );

  return (
    <div
      data-testid="status-filter"
      role="tablist"
      aria-label="Filter tasks by status"
      className="flex items-center"
      style={{ borderBottom: '1px solid var(--color-border-subtle)', gap: 0 }}
    >
      {TABS.map(({ key, label }) => {
        const active = statusFilter === key;
        const count = key === null ? totalCount : (counts[key] ?? 0);
        if (key !== null && count === 0) return null;

        return (
          <button
            key={label}
            onClick={() => setFilter(key)}
            role="tab"
            aria-selected={active}
            className="text-[14px] transition-colors"
            style={{
              whiteSpace: 'nowrap',
              background: 'transparent',
              color: active ? 'var(--color-text-1)' : 'var(--color-text-2)',
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
            {label}
            <span
              className="ml-1"
              style={{
                fontSize: 12,
                color: active ? 'var(--color-text-2)' : 'var(--color-text-3)',
              }}
            >
              {count}
            </span>
          </button>
        );
      })}
      <div className="ml-auto" style={{ paddingRight: 4 }}>
        <ViewOptions />
      </div>
    </div>
  );
}
