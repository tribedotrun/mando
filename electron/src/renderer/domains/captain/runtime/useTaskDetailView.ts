import { useCallback, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import { subscribeViewTaskBrief } from '#renderer/global/providers/viewBriefBus';
import {
  useTaskTimelineData,
  useTaskPrSummary,
  useTaskStop,
} from '#renderer/domains/captain/runtime/hooks';
import { invalidateTaskDetail } from '#renderer/domains/captain/repo/taskDetailInvalidation';
import { FINALIZED_STATUSES, type TaskItem, type SessionSummary } from '#renderer/global/types';
import { buildSessionsFromTimeline } from '#renderer/domains/sessions';

const REFRESH_INDICATOR_MS = 1500;

type DetailTab = 'feed' | 'pr' | 'terminal' | 'more';

interface Args {
  item: TaskItem;
  onBack: () => void;
  onOpenTranscript?: (opts: {
    sessionId: string;
    caller?: string;
    cwd?: string;
    project?: string;
    taskTitle?: string;
  }) => void;
  onResumeInTerminal?: (sessionId: string, name?: string) => void;
  activeTabProp: string | undefined;
}

export function useTaskDetailView({
  item,
  onBack,
  onOpenTranscript,
  onResumeInTerminal,
  activeTabProp,
}: Args) {
  const activeTab: DetailTab = (activeTabProp as DetailTab) || 'feed';
  const [prRefreshing, setPrRefreshing] = useState(false);
  const [contextModalOpen, setContextModalOpen] = useState(false);
  const queryClient = useQueryClient();
  const stopMut = useTaskStop();

  // Open the modal whenever the header overflow menu requests "View brief"
  // via the typed view-brief bus. The bus returns an unsubscribe function.
  useMountEffect(() => subscribeViewTaskBrief(() => setContextModalOpen(true)));

  const onBackRef = useRef(onBack);
  onBackRef.current = onBack;

  // Escape key handler.
  useMountEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      if (
        document.querySelector('[role="dialog"]') ||
        document.querySelector('[data-command-palette]') ||
        document.querySelector('[data-shortcut-overlay]')
      )
        return;
      e.stopPropagation();
      onBackRef.current();
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  });

  // Data queries.
  const { data: timelineData } = useTaskTimelineData(item.id);

  const isFinalized = FINALIZED_STATUSES.includes(item.status);
  const {
    data: prBody,
    isPending: prPending,
    refetch: refetchPr,
  } = useTaskPrSummary(item.id, item.pr_number ?? undefined, isFinalized);

  const events = timelineData?.events ?? [];
  const sessionMap = timelineData?.sessionMap ?? {};

  // Timeline is the authoritative source for session data.
  const sessions = buildSessionsFromTimeline(events, sessionMap, item);

  const navigateToTranscript = (sessionId: string, caller?: string, cwd?: string) => {
    onOpenTranscript?.({
      sessionId,
      caller: caller || 'worker',
      cwd: cwd || item.worktree || undefined,
      project: item.project || undefined,
      taskTitle: item.title || undefined,
    });
  };

  const handleSessionClick = (s: SessionSummary) => {
    navigateToTranscript(s.session_id, s.caller ?? undefined, s.cwd ?? item.worktree ?? undefined);
  };

  const handleResumeSession = useCallback(
    (sessionId: string, name?: string) => {
      onResumeInTerminal?.(sessionId, name);
    },
    [onResumeInTerminal],
  );

  const handleStop = async () => {
    try {
      await stopMut.mutateAsync({ id: item.id });
    } catch {
      // Toast is surfaced by useTaskStop's useMutationFeedback wrapper;
      // swallow here to avoid an unhandled rejection at the `void` callsite.
    } finally {
      void invalidateTaskDetail(queryClient, item.id);
    }
  };

  const handlePrRefresh = async () => {
    const startedAt = Date.now();
    setPrRefreshing(true);
    try {
      await refetchPr();
    } finally {
      const remaining = Math.max(0, REFRESH_INDICATOR_MS - (Date.now() - startedAt));
      window.setTimeout(() => setPrRefreshing(false), remaining);
    }
  };

  const tabs: { key: DetailTab; label: string }[] = [
    { key: 'feed', label: 'Feed' },
    { key: 'pr', label: 'PR' },
    { key: 'terminal', label: 'Terminal' },
    { key: 'more', label: 'More' },
  ];

  const validKeys = tabs.map((t) => t.key);
  const effectiveTab = validKeys.includes(activeTab) ? activeTab : 'feed';

  return {
    tabs: { items: tabs, effectiveTab },
    pr: {
      refreshing: prRefreshing,
      body: prBody,
      pending: prPending,
      handleRefresh: handlePrRefresh,
    },
    context: { open: contextModalOpen, setOpen: setContextModalOpen },
    sessions: { items: sessions, handleSessionClick, handleResumeSession },
    stop: { pending: stopMut.isPending, handle: handleStop },
  };
}
