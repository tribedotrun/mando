import React, { useCallback, useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import log from '#renderer/logger';
import {
  fetchTimeline,
  fetchItemSessions,
  fetchPrSummary,
} from '#renderer/domains/captain/hooks/useApi';
import { FINALIZED_STATUSES, type TaskItem, type SessionSummary } from '#renderer/types';
import { buildSessionsFromTimeline } from '#renderer/domains/sessions';
import { TaskActionBar } from '#renderer/domains/captain/components/TaskActionBar';
import { useTaskAsk } from '#renderer/global/hooks/useTaskAsk';
import {
  PrTab,
  SessionsTab,
  InfoTab,
  ContextModal,
} from '#renderer/domains/captain/components/TaskDetailTabs';
import { TaskFeedView } from '#renderer/domains/captain/components/TaskFeedView';
import { RefreshCw } from 'lucide-react';
import { cn } from '#renderer/cn';
import { Button } from '#renderer/components/ui/button';
import { Tabs, TabsList, TabsTrigger } from '#renderer/components/ui/tabs';
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
  TooltipProvider,
} from '#renderer/components/ui/tooltip';
import { queryKeys } from '#renderer/queryKeys';

type DetailTab = 'feed' | 'pr' | 'terminal' | 'more';
const REFRESH_INDICATOR_MS = 1500;

interface Props {
  item: TaskItem;
  onBack: () => void;
  onOpenTranscript?: (opts: {
    sessionId: string;
    caller?: string;
    cwd?: string;
    project?: string;
    taskTitle?: string;
  }) => void;
  activeTab?: string;
  onTabChange?: (tab: string) => void;
  onResumeInTerminal?: (sessionId: string, name?: string) => void;
  terminalSlot?: React.ReactNode;
}

export function TaskDetailView({
  item,
  onBack,
  onOpenTranscript,
  activeTab: activeTabProp,
  onTabChange,
  onResumeInTerminal,
  terminalSlot,
}: Props): React.ReactElement {
  const activeTab: DetailTab = (activeTabProp as DetailTab) || 'feed';
  const [prRefreshing, setPrRefreshing] = useState(false);
  const [contextModalOpen, setContextModalOpen] = useState(false);
  const { ask } = useTaskAsk(item.id);

  // Listen for header overflow menu triggering "View task brief"
  useMountEffect(() => {
    const handler = () => setContextModalOpen(true);
    document.addEventListener('mando:view-task-brief', handler);
    return () => document.removeEventListener('mando:view-task-brief', handler);
  });

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
  const { data: timelineData } = useQuery({
    queryKey: queryKeys.tasks.timeline(item.id),
    queryFn: async () => {
      const [tl, sess] = await Promise.all([fetchTimeline(item.id), fetchItemSessions(item.id)]);
      const map: Record<string, SessionSummary> = {};
      for (const s of sess.sessions) map[s.session_id] = s;
      return { events: tl.events, sessionMap: map, sessions: sess.sessions };
    },
  });

  const isFinalized = FINALIZED_STATUSES.includes(item.status);
  const {
    data: prBody,
    isPending: prPending,
    refetch: refetchPr,
  } = useQuery({
    queryKey: queryKeys.tasks.pr(item.id),
    queryFn: async () => {
      const data = await fetchPrSummary(item.id);
      // Persist to disk for finalized tasks.
      if (isFinalized && data.summary) {
        localStorage.setItem(`pr-cache:${item.id}`, JSON.stringify(data));
      }
      return data;
    },
    enabled: !!item.pr_number,
    staleTime: isFinalized ? Infinity : 30_000,
    initialData: () => {
      if (!isFinalized) return undefined;
      const key = `pr-cache:${item.id}`;
      const cached = localStorage.getItem(key);
      if (!cached) return undefined;
      try {
        return JSON.parse(cached);
      } catch (err) {
        log.warn(`[TaskDetail] corrupted pr-cache for item ${item.id}, clearing:`, err);
        localStorage.removeItem(key);
        return undefined;
      }
    },
  });

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
    navigateToTranscript(s.session_id, s.caller, s.cwd || item.worktree);
  };

  const handleResumeSession = useCallback(
    (sessionId: string, name?: string) => {
      onResumeInTerminal?.(sessionId, name);
    },
    [onResumeInTerminal],
  );

  const tabs: { key: DetailTab; label: string }[] = [
    { key: 'feed', label: 'Feed' },
    { key: 'pr', label: 'PR' },
    { key: 'terminal', label: 'Terminal' },
    { key: 'more', label: 'More' },
  ];

  const validKeys = tabs.map((t) => t.key);
  const effectiveTab = validKeys.includes(activeTab) ? activeTab : 'feed';

  return (
    <div className="flex h-full flex-col">
      {/* Main row */}
      <div className="flex min-h-0 flex-1">
        {/* Left column, entire column scrolls together */}
        <div
          className={cn(
            'min-h-0 min-w-0 flex-1 overflow-x-hidden',
            effectiveTab === 'feed' || effectiveTab === 'terminal'
              ? 'flex flex-col overflow-hidden'
              : 'scrollbar-on-hover overflow-y-auto',
          )}
        >
          <Tabs
            value={effectiveTab}
            onValueChange={(v) => onTabChange?.(v)}
            className={cn(
              'gap-0',
              (effectiveTab === 'feed' || effectiveTab === 'terminal') &&
                'flex flex-1 flex-col min-h-0',
            )}
          >
            <div className="sticky top-0 z-10 flex items-center justify-between bg-background">
              <TabsList variant="line" className="h-auto gap-0">
                {tabs.map((tab) => (
                  <TabsTrigger
                    key={tab.key}
                    value={tab.key}
                    className="px-3 py-1.5 text-caption font-medium"
                  >
                    {tab.label}
                  </TabsTrigger>
                ))}
              </TabsList>
              {effectiveTab === 'pr' && item.pr_number && (
                <TooltipProvider delayDuration={300}>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button
                        variant="ghost"
                        size="icon-xs"
                        disabled={prRefreshing}
                        onClick={() => {
                          setPrRefreshing(true);
                          void refetchPr();
                          setTimeout(() => setPrRefreshing(false), REFRESH_INDICATOR_MS);
                        }}
                        className="mr-2 text-text-3 hover:text-text-1"
                      >
                        <RefreshCw size={14} className={prRefreshing ? 'animate-spin' : ''} />
                        <span className="sr-only">Refresh PR</span>
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent side="bottom" className="text-xs">
                      Refresh PR
                    </TooltipContent>
                  </Tooltip>
                </TooltipProvider>
              )}
            </div>

            {/* Tab content */}
            {effectiveTab !== 'terminal' && (
              <div
                className={cn(
                  'break-words',
                  effectiveTab === 'feed' ? 'flex-1 min-h-0' : 'px-3 pt-3',
                )}
              >
                {effectiveTab === 'feed' && <TaskFeedView key={item.id} item={item} />}
                {effectiveTab === 'pr' && (
                  <PrTab item={item} prBody={prBody} prPending={prPending} />
                )}
                {effectiveTab === 'more' && (
                  <div className="space-y-6">
                    <InfoTab item={item} />
                    <SessionsTab
                      sessions={sessions}
                      onSessionClick={handleSessionClick}
                      onResumeSession={handleResumeSession}
                      taskId={item.id}
                    />
                  </div>
                )}
              </div>
            )}
            {/* Terminal stays mounted (display:none) to keep xterm alive */}
            {terminalSlot && (
              <div className={cn(effectiveTab === 'terminal' ? 'flex-1 min-h-0' : 'hidden')}>
                {terminalSlot}
              </div>
            )}
          </Tabs>
        </div>
      </div>

      {/* Action bar: only on PR and More tabs (feed has its own input, terminal doesn't need one) */}
      {effectiveTab !== 'feed' && effectiveTab !== 'terminal' && (
        <TaskActionBar item={item} onAsk={(q, images) => void ask(q, images)} />
      )}

      {/* Context modal */}
      {contextModalOpen && item.context && (
        <ContextModal context={item.context} onClose={() => setContextModalOpen(false)} />
      )}
    </div>
  );
}
