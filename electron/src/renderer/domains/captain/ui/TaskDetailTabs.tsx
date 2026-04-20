import React from 'react';
import type { TaskItem } from '#renderer/global/types';
import { shortenPath } from '#renderer/global/service/utils';
import { PrSections } from '#renderer/domains/captain/ui/PrSections';
import { Skeleton } from '#renderer/global/ui/skeleton';
import { CopyValue, ContextModal } from '#renderer/domains/captain/ui/TaskDetailTabsParts';

export { ContextModal };

/* -- PR tab -- */

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

/* -- Info tab -- */

export function InfoTab({ item }: { item: TaskItem }): React.ReactElement {
  return (
    <div className="space-y-5">
      <div className="grid grid-cols-[auto_1fr] items-baseline gap-x-6 gap-y-2.5">
        <span className="text-caption text-text-4">ID</span>
        <span className="font-mono text-caption text-text-2">#{item.id}</span>

        {item.worktree && (
          <>
            <span className="text-caption text-text-4">Worktree</span>
            <CopyValue value={item.worktree} display={shortenPath(item.worktree)} />
          </>
        )}

        {item.branch && (
          <>
            <span className="text-caption text-text-4">Branch</span>
            <CopyValue value={item.branch} />
          </>
        )}

        {item.plan && (
          <>
            <span className="text-caption text-text-4">Plan</span>
            <CopyValue value={item.plan} display={shortenPath(item.plan)} />
          </>
        )}

        {item.no_auto_merge && (
          <>
            <span className="text-caption text-text-4">Auto-merge</span>
            <span className="text-caption text-text-2">Disabled</span>
          </>
        )}
      </div>

      {item.original_prompt && (
        <div>
          <div className="mb-1.5 text-caption text-text-4">Original Request</div>
          <p className="text-body leading-relaxed text-text-2 [overflow-wrap:anywhere]">
            {item.original_prompt}
          </p>
        </div>
      )}
    </div>
  );
}
