import React, { useRef } from 'react';
import { useRouterState } from '@tanstack/react-router';
import {
  useWorkers,
  useResumeRateLimited,
  useTaskTimelineData,
} from '#renderer/domains/captain/runtime/hooks';
import { buildSessionsFromTimeline } from '#renderer/domains/sessions';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import { isRateLimited } from '#renderer/global/service/utils';
import { copyToClipboard } from '#renderer/global/runtime/useFeedback';
import { getPageTitle } from '#renderer/global/service/routeHelpers';
import { useWorkbenchCtx } from '#renderer/domains/captain/runtime/useWorkbenchCtx';

export interface AppHeaderData {
  ctx: ReturnType<typeof useWorkbenchCtx>;
  pathname: string;
  isTerminalTab: boolean;
  pageTitle: string;
  sessions: ReturnType<typeof buildSessionsFromTimeline>;
  rateLimitSecs: number;
  resumeMut: ReturnType<typeof useResumeRateLimited>;
  taskIsRateLimited: boolean;
}

export function useAppHeader(): AppHeaderData {
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

  return {
    ctx,
    pathname,
    isTerminalTab,
    pageTitle,
    sessions,
    rateLimitSecs,
    resumeMut,
    taskIsRateLimited,
  };
}
