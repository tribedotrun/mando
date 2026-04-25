import React from 'react';
import type { TaskItem } from '#renderer/global/types';
import { PrSections } from '#renderer/domains/captain/ui/PrSections';
import { Skeleton } from '#renderer/global/ui/primitives/skeleton';

export function PrTab({
  item,
  prBody,
  prPending,
}: {
  item: TaskItem;
  prBody: { summary: string | null } | undefined;
  prPending: boolean;
}): React.ReactElement {
  if (!item.pr_number) {
    return <div className="text-caption text-text-3">No PR associated with this task</div>;
  }
  if (prPending && !prBody) {
    return (
      <div className="min-h-[120px] space-y-3">
        <Skeleton className="h-4 w-3/4" />
        <Skeleton className="h-4 w-1/2" />
        <Skeleton className="h-4 w-2/3" />
      </div>
    );
  }
  if (!prBody?.summary) {
    return <div className="text-caption italic text-text-3">No PR description available</div>;
  }
  return <PrSections text={prBody.summary} />;
}
