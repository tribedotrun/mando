import React from 'react';
import { cn } from '#renderer/global/service/cn';
import { MergeIcon } from '#renderer/global/ui/primitives/icons';
import { DetailOverflowMenu } from '#renderer/domains/captain/ui/TaskDetailParts';
import { useUIStore } from '#renderer/global/runtime/useUIStore';
import { requestViewTaskBrief } from '#renderer/global/providers/viewBriefBus';
import { Button } from '#renderer/global/ui/primitives/button';
import { canMerge } from '#renderer/global/service/utils';
import { AppHeaderOpenMenu } from '#renderer/app/AppHeaderOpenMenu';
import {
  useTaskActions,
  type useResumeRateLimited,
  type useWorkbenchCtx,
} from '#renderer/domains/captain';

interface TaskTitleRowProps {
  ctx: NonNullable<ReturnType<typeof useWorkbenchCtx>>;
  sidebarCollapsed: boolean | undefined;
  navIcons: React.ReactNode;
  isTerminalTab: boolean;
  taskIsRateLimited: boolean;
  resumeMut: ReturnType<typeof useResumeRateLimited>;
}

export function TaskTitleRow({
  ctx,
  sidebarCollapsed,
  navIcons,
  isTerminalTab,
  taskIsRateLimited,
  resumeMut,
}: TaskTitleRowProps): React.ReactElement {
  const taskActions = useTaskActions();
  const taskId = ctx.task!.id;
  return (
    <div className={cn('flex min-w-0 items-center gap-3', sidebarCollapsed && 'pl-[70px]')}>
      {navIcons}
      <span className="min-w-0 truncate text-body font-medium text-foreground">
        {ctx.task!.title || ctx.task!.original_prompt || 'Untitled task'}
      </span>
      <span className="flex-1" />
      <div className="flex shrink-0 items-center gap-2" style={{ WebkitAppRegion: 'no-drag' }}>
        {canMerge(ctx.task!) && (
          <Button
            variant="ghost"
            size="icon-sm"
            className="bg-success-bg text-success"
            aria-label="Merge"
            title="Merge"
            data-testid="merge-btn"
            onClick={() => useUIStore.getState().setMergeItem(ctx.task!)}
          >
            <MergeIcon />
          </Button>
        )}
        {taskIsRateLimited && (
          <Button
            variant="outline"
            size="xs"
            disabled={resumeMut.isPending}
            onClick={() => resumeMut.mutate({ id: ctx.task!.id })}
          >
            {resumeMut.isPending ? 'Resuming...' : 'Resume'}
          </Button>
        )}
        <AppHeaderOpenMenu worktreePath={ctx.worktreePath} />
        <DetailOverflowMenu
          item={ctx.task!}
          onViewContext={isTerminalTab ? undefined : () => requestViewTaskBrief()}
          onCancel={() => taskActions.flow.handleCancel(taskId)}
        />
      </div>
    </div>
  );
}
