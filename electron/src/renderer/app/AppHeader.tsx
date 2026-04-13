import React, { useRef, useState } from 'react';
import { cn } from '#renderer/cn';
import { useRouterState } from '@tanstack/react-router';
import { useTaskList, useWorkbenchList, useWorkers } from '#renderer/hooks/queries';
import { useResumeRateLimited } from '#renderer/hooks/mutations';
import { ChevronDown, Copy, PanelLeft, ArrowLeft, ArrowRight, SquarePen } from 'lucide-react';
import { FinderIcon, CursorIcon, PrIcon, MergeIcon } from '#renderer/global/components/icons';
import { DetailOverflowMenu } from '#renderer/domains/captain/components/TaskDetailParts';
import { HeaderStatusBadge } from '#renderer/domains/captain/components/StatusCard';
import { buildSessionsFromTimeline } from '#renderer/domains/sessions';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import type { TaskItem, SessionSummary } from '#renderer/types';
import { queryKeys } from '#renderer/queryKeys';
import { useQuery } from '@tanstack/react-query';
import { fetchTimeline, fetchItemSessions } from '#renderer/domains/captain/hooks/useApi';
import { useUIStore } from '#renderer/app/uiStore';
import { Button } from '#renderer/components/ui/button';
import { Tooltip, TooltipTrigger, TooltipContent } from '#renderer/components/ui/tooltip';
import {
  copyToClipboard,
  getErrorMessage,
  isRateLimited,
  prLabel,
  prHref,
  prState,
  sortTaskItems,
} from '#renderer/utils';
import { toast } from 'sonner';
import { Kbd } from '#renderer/components/ui/kbd';

interface WorkbenchCtx {
  worktreeName: string | null;
  worktreePath: string | null;
  projectName: string | null;
  task: TaskItem | null;
}

/** Resolve the current workbench context from route state. */
function useWorkbenchCtx(): WorkbenchCtx | null {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const search = useRouterState({
    select: (s) => s.location.search as Record<string, string | undefined>,
  });

  const taskIdMatch = pathname.match(/^\/captain\/tasks\/(\d+)/);
  const taskId = taskIdMatch ? Number(taskIdMatch[1]) : null;
  const { data: taskData } = useTaskList();
  const task = taskId ? (taskData?.items.find((t) => t.id === taskId) ?? null) : null;

  const isTerminal = pathname === '/terminal';
  const terminalCwd = isTerminal ? (search.cwd ?? null) : null;
  const terminalProject = isTerminal ? (search.project ?? null) : null;
  const { data: workbenches = [] } = useWorkbenchList();

  return React.useMemo<WorkbenchCtx | null>(() => {
    if (task) {
      const wb = task.workbench_id
        ? workbenches.find((w) => w.id === task.workbench_id)
        : undefined;
      const wtPath = task.worktree ?? wb?.worktree ?? null;
      return {
        worktreeName: wtPath?.split('/').pop() ?? null,
        worktreePath: wtPath,
        projectName: task.project ?? null,
        task,
      };
    }

    if (isTerminal && terminalCwd) {
      const matchedWb = workbenches.find((wb) => wb.worktree === terminalCwd);
      const candidates =
        taskData?.items.filter(
          (t) => t.worktree === terminalCwd || (matchedWb && t.workbench_id === matchedWb.id),
        ) ?? [];
      const matchedTask = sortTaskItems(candidates)[0] ?? null;
      return {
        worktreeName: matchedWb?.title ?? terminalCwd.split('/').pop() ?? null,
        worktreePath: terminalCwd,
        projectName: matchedTask?.project ?? matchedWb?.project ?? terminalProject,
        task: matchedTask,
      };
    }

    return null;
  }, [task, isTerminal, terminalCwd, terminalProject, workbenches, taskData?.items]);
}

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
  const isTerminal = pathname === '/terminal';

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
  const { data: timelineData } = useQuery({
    queryKey: taskId != null ? queryKeys.tasks.timeline(taskId) : ['noop'],
    queryFn: async () => {
      const [tl, sess] = await Promise.all([fetchTimeline(taskId!), fetchItemSessions(taskId!)]);
      const map: Record<string, SessionSummary> = {};
      for (const s of sess.sessions) map[s.session_id] = s;
      return { events: tl.events, sessionMap: map, sessions: sess.sessions };
    },
    enabled: !!taskId,
  });
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

  // Derive page title for collapsed toolbar (non-task routes)
  const pageTitle = React.useMemo(() => {
    if (pathname.startsWith('/captain')) return 'Tasks';
    if (pathname.startsWith('/scout')) return 'Scout';
    if (pathname.startsWith('/sessions')) return 'Sessions';
    if (pathname.startsWith('/settings')) return 'Settings';
    if (pathname === '/terminal') return 'Terminal';
    return '';
  }, [pathname]);

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
          className="flex h-[38px] shrink-0 items-start pl-[70px] pt-[10px]"
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
      <div className="h-10 shrink-0" style={{ WebkitAppRegion: 'drag' } as React.CSSProperties} />
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
        <div className={cn('flex items-center gap-3', sidebarCollapsed && 'pl-[70px]')}>
          {navIcons}
          <span className="min-w-0 truncate text-body font-medium text-foreground">
            {ctx.task!.title || ctx.task!.original_prompt || 'Untitled task'}
          </span>
          <span className="flex-1" />
          <div
            className="flex shrink-0 items-center gap-2"
            style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
          >
            {ctx.task!.pr_number && ctx.task!.project && ctx.task!.status === 'awaiting-review' && (
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
                isTerminal
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
              <span className="ml-3 min-w-0 truncate text-body font-medium text-foreground">
                {ctx.worktreeName}
              </span>
            )}
          </div>
        )
      )}
      {/* Row 2: status + project + worktree */}
      <div
        className={cn(
          'flex items-center gap-2 text-caption text-text-3',
          (hasTask || (sidebarCollapsed && !hasTask)) && 'mt-2',
          sidebarCollapsed && 'pl-6',
        )}
      >
        {hasTask && <HeaderStatusBadge item={ctx.task!} sessions={sessions} />}
        {ctx.projectName && <span>{ctx.projectName}</span>}
        {ctx.projectName && ctx.worktreeName && <span>&middot;</span>}
        {ctx.worktreeName && <span>{ctx.worktreeName}</span>}
        {hasTask &&
          (ctx.worktreeName || ctx.projectName) &&
          ctx.task!.pr_number &&
          (ctx.task!.github_repo || ctx.task!.project) && <span>&middot;</span>}
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

function CollapsedNavIcons({
  onToggleSidebar,
  onGoBack,
  onGoForward,
  onNewTask,
}: {
  onToggleSidebar?: () => void;
  onGoBack?: () => void;
  onGoForward?: () => void;
  onNewTask?: () => void;
}): React.ReactElement {
  return (
    <div
      className="flex shrink-0 items-center gap-1"
      style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
    >
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            onClick={onToggleSidebar}
            aria-label="Toggle sidebar"
            className="flex h-6 w-6 items-center justify-center rounded text-text-3 transition-colors hover:text-muted-foreground"
          >
            <PanelLeft size={14} />
          </button>
        </TooltipTrigger>
        <TooltipContent
          side="bottom"
          className="flex items-center gap-3 px-3 py-2 text-sm font-medium"
        >
          Toggle sidebar <Kbd>&#8984;B</Kbd>
        </TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            onClick={onGoBack}
            aria-label="Back"
            className="flex h-6 w-6 items-center justify-center rounded text-text-3 transition-colors hover:text-muted-foreground"
          >
            <ArrowLeft size={14} />
          </button>
        </TooltipTrigger>
        <TooltipContent
          side="bottom"
          className="flex items-center gap-3 px-3 py-2 text-sm font-medium"
        >
          Back <Kbd>&#8984;[</Kbd>
        </TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            onClick={onGoForward}
            aria-label="Forward"
            className="flex h-6 w-6 items-center justify-center rounded text-text-3 transition-colors hover:text-muted-foreground"
          >
            <ArrowRight size={14} />
          </button>
        </TooltipTrigger>
        <TooltipContent
          side="bottom"
          className="flex items-center gap-3 px-3 py-2 text-sm font-medium"
        >
          Forward <Kbd>&#8984;]</Kbd>
        </TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            onClick={onNewTask}
            aria-label="New task"
            className="flex h-6 w-6 items-center justify-center rounded text-text-3 transition-colors hover:text-muted-foreground"
          >
            <SquarePen size={14} />
          </button>
        </TooltipTrigger>
        <TooltipContent side="bottom" className="px-3 py-2 text-sm font-medium">
          New task
        </TooltipContent>
      </Tooltip>
    </div>
  );
}

function openPath(fn: () => Promise<void>, label: string) {
  fn().catch((err) => toast.error(getErrorMessage(err, `Failed to open in ${label}`)));
}

function OpenMenu({ worktreePath }: { worktreePath: string | null }): React.ReactElement {
  const [open, setOpen] = useState(false);
  const disabled = !worktreePath;

  useMountEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setOpen((prev) => {
          if (prev) e.stopPropagation();
          return false;
        });
      }
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  });

  return (
    <div className="relative" style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}>
      {/* Split button: default action (Cursor) + dropdown chevron */}
      <div
        className={cn(
          'flex items-center rounded-md border border-border',
          disabled && 'opacity-40 pointer-events-none',
        )}
      >
        <button
          onClick={() =>
            worktreePath && openPath(() => window.mandoAPI.openInCursor(worktreePath), 'Cursor')
          }
          disabled={disabled}
          className="flex items-center rounded-l-md px-2 py-1 transition-colors hover:bg-accent"
          aria-label="Open in Cursor"
        >
          <CursorIcon size={14} />
        </button>
        <div className="h-4 w-px bg-border" />
        <button
          onClick={() => !disabled && setOpen((v) => !v)}
          disabled={disabled}
          className="flex items-center rounded-r-md px-1.5 py-1 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          aria-label="More open options"
          aria-haspopup="true"
          aria-expanded={open}
        >
          <ChevronDown size={12} />
        </button>
      </div>
      {open && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} />
          <div className="absolute right-0 top-full z-50 mt-1 min-w-[200px] rounded-md border border-border bg-popover py-1 shadow-lg">
            <button
              className="flex w-full items-center gap-2.5 px-3 py-2 text-[13px] text-popover-foreground transition-colors hover:bg-accent"
              onClick={() => {
                openPath(() => window.mandoAPI.openInFinder(worktreePath!), 'Finder');
                setOpen(false);
              }}
            >
              <FinderIcon size={16} />
              <span className="flex-1 text-left">Finder</span>
            </button>
            <button
              className="flex w-full items-center gap-2.5 px-3 py-2 text-[13px] text-popover-foreground transition-colors hover:bg-accent"
              onClick={() => {
                openPath(() => window.mandoAPI.openInCursor(worktreePath!), 'Cursor');
                setOpen(false);
              }}
            >
              <CursorIcon size={16} />
              <span className="flex-1 text-left">Cursor</span>
            </button>
            <div className="my-1 h-px bg-border" />
            <button
              className="flex w-full items-center gap-2.5 px-3 py-2 text-[13px] text-popover-foreground transition-colors hover:bg-accent"
              onClick={() => {
                void copyToClipboard(worktreePath!, 'Path copied');
                setOpen(false);
              }}
            >
              <Copy size={15} className="shrink-0 text-muted-foreground" />
              <span className="flex-1 text-left">Copy path</span>
              <span className="flex items-center gap-0.5 text-text-3">
                <Kbd>&#8984;</Kbd>
                <Kbd>&#8679;</Kbd>
                <Kbd>C</Kbd>
              </span>
            </button>
          </div>
        </>
      )}
    </div>
  );
}
