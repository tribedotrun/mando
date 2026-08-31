import React from 'react';
import { cn } from '#renderer/global/service/cn';
import { PrIcon, MergeIcon } from '#renderer/global/ui/primitives/icons';
import { TaskStatusBadge } from '#renderer/domains/captain/ui/TaskStatusBadge';
import { prLabel, prHref, prState, canMerge } from '#renderer/global/service/utils';
import { AppHeaderOpenMenu } from '#renderer/app/AppHeaderOpenMenu';
import { type useWorkbenchCtx, useTaskActions } from '#renderer/domains/captain';
import type { buildSessionsFromTimeline } from '#renderer/domains/sessions';
import { DetailOverflowMenu } from '#renderer/domains/captain/ui/TaskDetailParts';
import { useUIStore } from '#renderer/global/runtime/useUIStore';
import { requestViewTaskBrief } from '#renderer/global/providers/viewBriefBus';
import { Button } from '#renderer/global/ui/primitives/button';

interface HeaderMetaRowProps {
  ctx: NonNullable<ReturnType<typeof useWorkbenchCtx>>;
  hasTask: boolean;
  sidebarCollapsed: boolean | undefined;
  sessions: ReturnType<typeof buildSessionsFromTimeline>;
}

export function HeaderMetaRow({
  ctx,
  hasTask,
  sidebarCollapsed,
  sessions,
}: HeaderMetaRowProps): React.ReactElement {
  return (
    <div
      className={cn(
        'flex min-w-0 items-center gap-2 text-caption text-text-3',
        (hasTask || (sidebarCollapsed && !hasTask)) && 'mt-2',
        sidebarCollapsed && 'pl-6',
      )}
    >
      {hasTask && (
        <span className="shrink-0">
          <TaskStatusBadge item={ctx.task!} sessions={sessions} />
        </span>
      )}
      {ctx.projectName && (
        <span className="max-w-[200px] truncate" title={ctx.projectName}>
          {ctx.projectName}
        </span>
      )}
      {ctx.projectName && ctx.worktreeName && <span className="shrink-0">&middot;</span>}
      {ctx.worktreeName && (
        <span className="min-w-0 truncate" title={ctx.worktreeName}>
          {ctx.worktreeName}
        </span>
      )}
      {hasTask &&
        (ctx.worktreeName || ctx.projectName) &&
        ctx.task!.pr_number &&
        (ctx.task!.github_repo || ctx.task!.project) && <span className="shrink-0">&middot;</span>}
      {hasTask && ctx.task!.pr_number && (ctx.task!.github_repo || ctx.task!.project) && (
        <a
          href={prHref(ctx.task!.pr_number, (ctx.task!.github_repo ?? ctx.task!.project)!)}
          target="_blank"
          rel="noopener noreferrer"
          className="inline-flex shrink-0 items-center gap-0.5 font-mono text-[11px] text-text-3 hover:text-foreground"
          style={{ WebkitAppRegion: 'no-drag' }}
        >
          <PrIcon state={prState(ctx.task!.status)} />
          {prLabel(ctx.task!.pr_number)}
        </a>
      )}
      {!hasTask && (
        <>
          <span className="flex-1" />
          {ctx.worktreePath && (
            <div style={{ WebkitAppRegion: 'no-drag' }}>
              <AppHeaderOpenMenu worktreePath={ctx.worktreePath} />
            </div>
          )}
        </>
      )}
    </div>
  );
}

interface TaskTitleRowProps {
  ctx: NonNullable<ReturnType<typeof useWorkbenchCtx>>;
  sidebarCollapsed: boolean | undefined;
  navIcons: React.ReactNode;
  taskIsRateLimited: boolean;
}

export function TaskTitleRow({
  ctx,
  sidebarCollapsed,
  navIcons,
  taskIsRateLimited,
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
            disabled={taskActions.flow.resumeRateLimitedPending}
            onClick={() => taskActions.flow.handleResumeRateLimited(taskId)}
          >
            {taskActions.flow.resumeRateLimitedPending ? 'Resuming...' : 'Resume'}
          </Button>
        )}
        <AppHeaderOpenMenu worktreePath={ctx.worktreePath} />
        <DetailOverflowMenu
          item={ctx.task!}
          onViewContext={() => requestViewTaskBrief()}
          onResumeRateLimited={
            taskIsRateLimited ? () => taskActions.flow.handleResumeRateLimited(taskId) : undefined
          }
          resumeRateLimitedPending={taskActions.flow.resumeRateLimitedPending}
          onCancel={() => taskActions.flow.handleCancel(taskId)}
        />
      </div>
    </div>
  );
}
