import React from 'react';
import * as Tabs from '@radix-ui/react-tabs';
import { cn } from '#renderer/cn';

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
    <Tabs.Root data-testid="scout-status-tabs" value={activeStatus} onValueChange={onStatusChange}>
      <Tabs.List className="flex items-center gap-0 border-b border-border-subtle">
        {STATUSES.map((s) => {
          const count = s === 'all' ? allCount : (statusCounts[s] ?? 0);
          const isError = s === 'error' && count > 0;
          return (
            <Tabs.Trigger
              key={s}
              value={s}
              className={cn(
                'group text-[13px] px-3 py-1.5 -mb-px border-b-2 transition-colors',
                'bg-transparent cursor-pointer',
                isError
                  ? 'text-error border-transparent data-[state=active]:border-error data-[state=active]:font-medium data-[state=inactive]:font-normal'
                  : 'data-[state=active]:border-accent data-[state=active]:text-text-1 data-[state=active]:font-medium data-[state=inactive]:border-transparent data-[state=inactive]:text-text-2 data-[state=inactive]:font-normal',
              )}
            >
              {s}
              {count > 0 && (
                <span className="ml-1 text-xs text-text-3 group-data-[state=active]:text-text-2">
                  {count}
                </span>
              )}
            </Tabs.Trigger>
          );
        })}
      </Tabs.List>
    </Tabs.Root>
  );
}
