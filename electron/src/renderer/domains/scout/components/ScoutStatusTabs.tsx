import React from 'react';
import { Tabs, TabsList, TabsTrigger } from '#renderer/components/ui/tabs';
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
    <Tabs data-testid="scout-status-tabs" value={activeStatus} onValueChange={onStatusChange}>
      <TabsList variant="line" className="w-full justify-start">
        {STATUSES.map((s) => {
          const count = s === 'all' ? allCount : (statusCounts[s] ?? 0);
          const isError = s === 'error' && count > 0;
          return (
            <TabsTrigger
              key={s}
              value={s}
              className={cn(
                'text-[13px]',
                isError && 'text-destructive data-[state=active]:text-destructive',
              )}
            >
              {s}
              {count > 0 && <span className="ml-1 text-xs opacity-60">{count}</span>}
            </TabsTrigger>
          );
        })}
      </TabsList>
    </Tabs>
  );
}
