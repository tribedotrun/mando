import React, { useRef, useState } from 'react';
import { cn } from '#renderer/cn';
import { useRouterState } from '@tanstack/react-router';
import { useTaskList, useWorkbenchList } from '#renderer/hooks/queries';
import { GitBranch, ChevronDown, Copy } from 'lucide-react';
import { FinderIcon, CursorIcon } from '#renderer/global/components/icons';
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
import { copyToClipboard, getErrorMessage } from '#renderer/utils';
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
    if (task?.worktree || task?.workbench_id) {
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
      return {
        worktreeName: matchedWb?.title ?? terminalCwd.split('/').pop() ?? null,
        worktreePath: terminalCwd,
        projectName: terminalProject,
        task: null,
      };
    }

    return null;
  }, [task, isTerminal, terminalCwd, terminalProject, workbenches]);
}

export function AppHeader(): React.ReactElement {
  const ctx = useWorkbenchCtx();

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

  if (!ctx) {
    return (
      <div className="h-10 shrink-0" style={{ WebkitAppRegion: 'drag' } as React.CSSProperties} />
    );
  }

  const hasTask = !!ctx.task;

  return (
    <div
      className={cn(
        'flex shrink-0 flex-col justify-center border-b border-border px-6',
        hasTask ? 'py-2' : 'h-10',
      )}
      style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}
    >
      {hasTask ? (
        <>
          {/* Row 1: title + actions */}
          <div className="flex items-center gap-3">
            <span className="min-w-0 truncate text-body font-medium text-foreground">
              {ctx.task!.title}
            </span>
            <span className="flex-1" />
            <div
              className="flex shrink-0 items-center gap-2"
              style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
            >
              {ctx.task!.pr_number &&
                ctx.task!.project &&
                ctx.task!.status === 'awaiting-review' && (
                  <Button size="sm" onClick={() => useUIStore.getState().setMergeItem(ctx.task!)}>
                    Merge
                  </Button>
                )}
              {ctx.worktreePath && <OpenMenu worktreePath={ctx.worktreePath} />}
              {(ctx.task!.branch || ctx.task!.worktree || ctx.task!.plan || ctx.task!.context) && (
                <DetailOverflowMenu
                  item={ctx.task!}
                  onViewContext={() =>
                    document.dispatchEvent(new CustomEvent('mando:view-task-brief'))
                  }
                />
              )}
            </div>
          </div>
          {/* Row 2: status + project + worktree */}
          <div className="mt-2 flex items-center gap-2 text-caption text-text-3">
            <HeaderStatusBadge item={ctx.task!} sessions={sessions} />
            {ctx.projectName && <span>{ctx.projectName}</span>}
            {ctx.projectName && ctx.worktreeName && <span>&middot;</span>}
            {ctx.worktreeName && (
              <span className="flex items-center gap-1">
                <GitBranch size={11} className="shrink-0" />
                {ctx.worktreeName}
              </span>
            )}
          </div>
        </>
      ) : (
        /* Non-task pages: single-line layout */
        <div className="flex items-center gap-3">
          {ctx.worktreeName && (
            <span
              className="flex items-center gap-1.5 text-caption text-text-3"
              style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
            >
              <GitBranch size={12} className="shrink-0" />
              <span>{ctx.worktreeName}</span>
            </span>
          )}
          {ctx.projectName && <span className="text-caption text-text-3">{ctx.projectName}</span>}
          <span className="flex-1" />
          {ctx.worktreePath && (
            <div style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}>
              <OpenMenu worktreePath={ctx.worktreePath} />
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function openPath(fn: () => Promise<void>, label: string) {
  fn().catch((err) => toast.error(getErrorMessage(err, `Failed to open in ${label}`)));
}

function OpenMenu({ worktreePath }: { worktreePath: string }): React.ReactElement {
  const [open, setOpen] = useState(false);

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
      <div className="flex items-center rounded-md border border-border">
        <button
          onClick={() => openPath(() => window.mandoAPI.openInCursor(worktreePath), 'Cursor')}
          className="flex items-center rounded-l-md px-2 py-1 transition-colors hover:bg-accent"
          aria-label="Open in Cursor"
        >
          <CursorIcon size={14} />
        </button>
        <div className="h-4 w-px bg-border" />
        <button
          onClick={() => setOpen((v) => !v)}
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
                openPath(() => window.mandoAPI.openInFinder(worktreePath), 'Finder');
                setOpen(false);
              }}
            >
              <FinderIcon size={16} />
              <span className="flex-1 text-left">Finder</span>
            </button>
            <button
              className="flex w-full items-center gap-2.5 px-3 py-2 text-[13px] text-popover-foreground transition-colors hover:bg-accent"
              onClick={() => {
                openPath(() => window.mandoAPI.openInCursor(worktreePath), 'Cursor');
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
                void copyToClipboard(worktreePath, 'Path copied');
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
