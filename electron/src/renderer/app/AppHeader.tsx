import React, { useRef } from 'react';
import { cn } from '#renderer/global/service/cn';
import { useRouterState } from '@tanstack/react-router';
import { useWorkers, useResumeRateLimited, useTaskTimelineData } from '#renderer/domains/captain';
import { PrIcon, MergeIcon } from '#renderer/global/ui/icons';
import { DetailOverflowMenu } from '#renderer/domains/captain/ui/TaskDetailParts';
import { HeaderStatusBadge } from '#renderer/domains/captain/ui/TaskStatusBadge';
import { buildSessionsFromTimeline } from '#renderer/domains/sessions';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import { useUIStore } from '#renderer/app/uiStore';
import { Button } from '#renderer/global/ui/button';
import {
  canMerge,
  copyToClipboard,
  isRateLimited,
  prLabel,
  prHref,
  prState,
} from '#renderer/global/service/utils';
import { getPageTitle } from '#renderer/global/service/routeHelpers';
import { useWorkbenchCtx } from '#renderer/app/useWorkbenchCtx';
import { CollapsedNavIcons, OpenMenu } from '#renderer/app/AppHeaderParts';

interface AppHeaderProps {
  sidebarCollapsed?: boolean;
  onToggleSidebar?: () => void;
  onGoBack?: () => void;
  onGoForward?: () => void;
  onNewTask?: () => void;
}

export function AppHeader({
  sidebarCollapsed,
  onToggleSidebar,
  onGoBack,
  onGoForward,
  onNewTask,
}: AppHeaderProps): React.ReactElement {
  const ctx = useWorkbenchCtx();
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const search = useRouterState({
    select: (s) => s.location.search as Record<string, string | undefined>,
  });
  const isTerminalTab = pathname.startsWith('/wb/') && search.tab === 'terminal';

  // Cmd+Shift+C copies the worktree path (ref avoids stale closure)
  const worktreePathRef = useRef(ctx?.worktreePath);
  worktreePathRef.current = ctx?.worktreePath;

  useMountEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const p = worktreePathRef.current;
      if (!p) return;
      if (e.metaKey && e.shiftKey && e.key.toLowerCase() === 'c') {
        e.preventDefault();
        void copyToClipboard(p, 'Path copied');
      }
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  });

  // Fetch sessions for the status badge when viewing a task.
  const taskId = ctx?.task?.id ?? null;
  const { data: timelineData } = useTaskTimelineData(taskId ?? 0);
  const sessions = React.useMemo(
    () =>
      ctx?.task && timelineData
        ? buildSessionsFromTimeline(timelineData.events, timelineData.sessionMap, ctx.task)
        : [],
    [ctx?.task, timelineData],
  );

  // Rate-limit resume for task detail header
  const { data: workersData } = useWorkers();
  const rateLimitSecs = workersData?.rate_limit_remaining_secs ?? 0;
  const resumeMut = useResumeRateLimited();
  const taskIsRateLimited = ctx?.task ? isRateLimited(ctx.task, rateLimitSecs) : false;

  const pageTitle = getPageTitle(pathname);

  const navIcons = sidebarCollapsed ? (
    <CollapsedNavIcons
      onToggleSidebar={onToggleSidebar}
      onGoBack={onGoBack}
      onGoForward={onGoForward}
      onNewTask={onNewTask}
    />
  ) : null;

  if (!ctx) {
    if (sidebarCollapsed) {
      return (
        <div
          className="relative z-10 flex h-[38px] shrink-0 items-start bg-background pl-[70px] pt-[10px]"
          style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}
        >
          {navIcons}
          <span className="ml-3 min-w-0 truncate text-body font-medium text-foreground">
            {pageTitle}
          </span>
        </div>
      );
    }
    return (
      <div
        className="relative z-10 h-10 shrink-0 bg-background"
        style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}
      />
    );
  }

  const hasTask = !!ctx.task;

  return (
    <div
      className={cn(
        'flex shrink-0 flex-col justify-center border-b border-border',
        sidebarCollapsed ? 'pb-2 pr-6 pt-[10px]' : 'px-6 py-2',
      )}
      style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}
    >
      {hasTask ? (
        <div className={cn('flex min-w-0 items-center gap-3', sidebarCollapsed && 'pl-[70px]')}>
          {navIcons}
          <span className="min-w-0 truncate text-body font-medium text-foreground">
            {ctx.task!.title || ctx.task!.original_prompt || 'Untitled task'}
          </span>
          <span className="flex-1" />
          <div
            className="flex shrink-0 items-center gap-2"
            style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
          >
            {canMerge(ctx.task!) && (
              <Button
                variant="ghost"
                size="icon-sm"
                className="bg-success-bg text-success"
                aria-label="Merge"
                title="Merge"
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
            <OpenMenu worktreePath={ctx.worktreePath} />
            <DetailOverflowMenu
              item={ctx.task!}
              onViewContext={
                isTerminalTab
                  ? undefined
                  : () => document.dispatchEvent(new CustomEvent('mando:view-task-brief'))
              }
            />
          </div>
        </div>
      ) : (
        sidebarCollapsed && (
          <div className="flex items-center pl-[70px]">
            {navIcons}
            {ctx.worktreeName && (
              <span
                className="ml-3 min-w-0 truncate text-body font-medium text-foreground"
                title={ctx.worktreeName}
              >
                {ctx.worktreeName}
              </span>
            )}
          </div>
        )
      )}
      {/* Row 2: status + project + worktree */}
      <div
        className={cn(
          'flex min-w-0 items-center gap-2 text-caption text-text-3',
          (hasTask || (sidebarCollapsed && !hasTask)) && 'mt-2',
          sidebarCollapsed && 'pl-6',
        )}
      >
        {hasTask && (
          <span className="shrink-0">
            <HeaderStatusBadge item={ctx.task!} sessions={sessions} />
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
          (ctx.task!.github_repo || ctx.task!.project) && (
            <span className="shrink-0">&middot;</span>
          )}
        {hasTask && ctx.task!.pr_number && (ctx.task!.github_repo || ctx.task!.project) && (
          <a
            href={prHref(ctx.task!.pr_number, (ctx.task!.github_repo ?? ctx.task!.project)!)}
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex shrink-0 items-center gap-0.5 font-mono text-[11px] text-text-3 hover:text-foreground"
            style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
          >
            <PrIcon state={prState(ctx.task!.status)} />
            {prLabel(ctx.task!.pr_number)}
          </a>
        )}
        {!hasTask && (
          <>
            <span className="flex-1" />
            {ctx.worktreePath && (
              <div style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}>
                <OpenMenu worktreePath={ctx.worktreePath} />
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
