import React from 'react';
import type { ItemStatus } from '#renderer/types';
import { ACTION_NEEDED_STATUSES, IN_PROGRESS_STATUSES } from '#renderer/types';
import { useTaskStore } from '#renderer/domains/captain/stores/taskStore';
import { useProjectFilterPaths } from '#renderer/domains/settings';
import { ViewOptions } from '#renderer/domains/captain/components/ViewOptions';
import { Button } from '#renderer/components/ui/button';

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
  const filterPaths = useProjectFilterPaths(projectFilter);

  const filtered = React.useMemo(
    () => (filterPaths ? items.filter((i) => i.project && filterPaths.has(i.project)) : items),
    [items, filterPaths],
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
      className="flex items-center gap-0"
    >
      {TABS.map(({ key, label }) => {
        const active = statusFilter === key;
        const count = key === null ? totalCount : (counts[key] ?? 0);
        if (key !== null && count === 0) return null;

        return (
          <Button
            key={label}
            variant="ghost"
            onClick={() => setFilter(key)}
            role="tab"
            aria-selected={active}
            className={`text-body whitespace-nowrap rounded-none bg-transparent px-3 py-2 transition-colors -mb-px border-b-2 ${active ? 'border-primary font-medium text-foreground' : 'border-transparent font-normal text-muted-foreground'}`}
          >
            {label}
            <span
              className={`ml-1 text-[12px] ${active ? 'text-muted-foreground' : 'text-text-3'}`}
            >
              {count}
            </span>
          </Button>
        );
      })}
      <div className="ml-auto pr-1">
        <ViewOptions />
      </div>
    </div>
  );
}
