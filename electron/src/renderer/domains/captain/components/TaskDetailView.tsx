import React, { useCallback, useMemo, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import log from '#renderer/logger';
import { toast } from 'sonner';
import {
  fetchTimeline,
  fetchItemSessions,
  fetchPrSummary,
  fetchAskHistory,
} from '#renderer/domains/captain/hooks/useApi';
import {
  FINALIZED_STATUSES,
  type TaskItem,
  type SessionSummary,
  type TimelineEvent,
  type MandoConfig,
} from '#renderer/types';
import { extractClarifierQuestions } from '#renderer/utils';
import { buildSessionsFromTimeline } from '#renderer/domains/sessions';
import { TaskActionBar } from '#renderer/domains/captain/components/TaskActionBar';
import {
  EscalatedReportTab,
  ClarificationTab,
} from '#renderer/domains/captain/components/StatusCard';
import { QATab } from '#renderer/domains/captain/components/TaskQAView';
import {
  TimelineTab,
  PrTab,
  SessionsTab,
  InfoTab,
  ContextModal,
} from '#renderer/domains/captain/components/TaskDetailTabs';
import { RefreshCw } from 'lucide-react';
import { Button } from '#renderer/components/ui/button';
import { Tabs, TabsList, TabsTrigger } from '#renderer/components/ui/tabs';
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
  TooltipProvider,
} from '#renderer/components/ui/tooltip';
import { queryKeys } from '#renderer/queryKeys';

type DetailTab =
  | 'escalated'
  | 'respond'
  | 'timeline'
  | 'pr'
  | 'sessions'
  | 'info'
  | 'qa'
  | 'terminal';
const REFRESH_INDICATOR_MS = 1500;

interface Props {
  item: TaskItem;
  onBack: () => void;
  onOpenTerminal?: (opts: {
    project: string;
    cwd: string;
    resumeSessionId?: string;
    name?: string;
  }) => void;
  onOpenTranscript?: (opts: {
    sessionId: string;
    caller?: string;
    cwd?: string;
    project?: string;
    taskTitle?: string;
  }) => void;
}

export function TaskDetailView({
  item,
  onBack,
  onOpenTerminal,
  onOpenTranscript,
}: Props): React.ReactElement {
  const qc = useQueryClient();
  const [activeTab, setActiveTab] = useState<DetailTab>(() => {
    if (item.status === 'escalated') return 'escalated';
    if (item.status === 'needs-clarification') return 'respond';
    return 'pr';
  });
  const [prRefreshing, setPrRefreshing] = useState(false);
  const [contextModalOpen, setContextModalOpen] = useState(false);
  const [pendingQuestion, setPendingQuestion] = useState<string | null>(null);

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

  useQuery({
    queryKey: queryKeys.tasks.askHistory(item.id),
    queryFn: () => fetchAskHistory(item.id),
  });
  const events = timelineData?.events ?? [];
  const sessionMap = timelineData?.sessionMap ?? {};

  // Timeline is the authoritative source for session data.
  const sessions = buildSessionsFromTimeline(events, sessionMap, item);

  // Extract clarifier questions from latest timeline event.
  const clarifierQuestions = useMemo(
    () => extractClarifierQuestions(events, item.status),
    [events, item.status],
  );

  const navigateToTranscript = (sessionId: string, caller?: string, cwd?: string) => {
    onOpenTranscript?.({
      sessionId,
      caller: caller || 'worker',
      cwd: cwd || item.worktree || undefined,
      project: item.project || undefined,
      taskTitle: item.title || undefined,
    });
  };

  const handleTranscriptClick = (sessionId: string, _event: TimelineEvent) => {
    const summary = sessionMap[sessionId];
    navigateToTranscript(sessionId, summary?.caller, summary?.cwd || item.worktree);
  };

  const handleSessionClick = (s: SessionSummary) => {
    navigateToTranscript(s.session_id, s.caller, s.cwd || item.worktree);
  };

  const handleAskFromBar = useCallback((question: string) => {
    setActiveTab('qa');
    setPendingQuestion(question);
  }, []);

  const openTerminalPage = useCallback(
    (resumeId?: string, name?: string, sessionCwd?: string) => {
      if (!onOpenTerminal || !item.project) return;

      const cfg = qc.getQueryData<MandoConfig>(queryKeys.config.current());
      const projectPath = cfg?.captain?.projects
        ? Object.values(cfg.captain.projects).find((p) => p.name === item.project)?.path
        : undefined;

      // Resume uses the session's stored cwd -- Claude Code resumes by session
      // ID, so the directory just needs to exist on disk.
      // New sessions use the worktree for correct git context.
      const cwd = resumeId
        ? sessionCwd || projectPath || item.worktree
        : (item.worktree ?? projectPath);

      if (!cwd) {
        toast.error(`No working directory for task "${item.title}"`);
        return;
      }
      onOpenTerminal({
        project: item.project,
        cwd,
        resumeSessionId: resumeId,
        name,
      });
    },
    [onOpenTerminal, item.project, item.worktree, item.title, qc],
  );

  const handleResumeSession = useCallback(
    (sessionId: string, name?: string, sessionCwd?: string) =>
      openTerminalPage(sessionId, name, sessionCwd),
    [openTerminalPage],
  );

  // Dynamic action tabs that appear at the front when status demands it.
  const showEscalated = item.status === 'escalated';
  const showRespond =
    item.status === 'needs-clarification' &&
    clarifierQuestions != null &&
    clarifierQuestions.length > 0;

  const tabs = useMemo(() => {
    const base: { key: DetailTab; label: string; accent?: boolean }[] = [
      { key: 'pr', label: 'PR' },
      { key: 'timeline', label: 'Timeline' },
      { key: 'sessions', label: 'Sessions' },
      { key: 'info', label: 'Info' },
      { key: 'qa', label: 'Q&A' },
      { key: 'terminal', label: 'Terminal' },
    ];
    if (showRespond) base.unshift({ key: 'respond', label: 'Respond', accent: true });
    if (showEscalated) base.unshift({ key: 'escalated', label: 'Report', accent: true });
    return base;
  }, [showEscalated, showRespond]);

  // Auto-select action tabs when status transitions (not on mount -- useState init handles that).
  const prevStatus = useRef(item.status);
  if (item.status !== prevStatus.current) {
    prevStatus.current = item.status;
    if (showEscalated) setActiveTab('escalated');
    else if (showRespond) setActiveTab('respond');
  }

  // If the active tab disappears (status changed), fall back to PR.
  const validKeys = tabs.map((t) => t.key);
  const effectiveTab = validKeys.includes(activeTab) ? activeTab : 'pr';

  return (
    <div className="flex h-full flex-col">
      {/* Main row */}
      <div className="flex min-h-0 flex-1">
        {/* Left column, entire column scrolls together */}
        <div className="min-h-0 min-w-0 flex-1 overflow-y-auto overflow-x-hidden">
          <Tabs
            value={effectiveTab}
            onValueChange={(v) => {
              if (v === 'terminal') {
                openTerminalPage();
              } else {
                setActiveTab(v as DetailTab);
              }
            }}
            className="gap-0"
          >
            <div className="sticky top-0 z-10 flex items-center justify-between bg-background">
              <TabsList variant="line" className="h-auto gap-0">
                {tabs.map((tab) => (
                  <TabsTrigger
                    key={tab.key}
                    value={tab.key}
                    className={
                      tab.accent
                        ? 'px-3 py-1.5 text-caption font-medium text-[var(--needs-human)]'
                        : 'px-3 py-1.5 text-caption font-medium'
                    }
                  >
                    {tab.accent && <span className="mr-1">!</span>}
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
            <div className="break-words pr-2 pt-4">
              {effectiveTab === 'escalated' && <EscalatedReportTab item={item} />}
              {effectiveTab === 'respond' && clarifierQuestions && (
                <ClarificationTab taskId={item.id} questions={clarifierQuestions} />
              )}
              {effectiveTab === 'timeline' && (
                <TimelineTab events={events} onTranscriptClick={handleTranscriptClick} />
              )}
              {effectiveTab === 'pr' && <PrTab item={item} prBody={prBody} prPending={prPending} />}
              {effectiveTab === 'sessions' && (
                <SessionsTab
                  sessions={sessions}
                  onSessionClick={handleSessionClick}
                  onResumeSession={handleResumeSession}
                  taskId={item.id}
                />
              )}
              {effectiveTab === 'info' && <InfoTab item={item} />}
              {effectiveTab === 'qa' && (
                <QATab
                  item={item}
                  pendingQuestion={pendingQuestion}
                  onPendingConsumed={() => setPendingQuestion(null)}
                />
              )}
            </div>
          </Tabs>
        </div>
      </div>

      {/* Action bar, pinned at bottom */}
      <TaskActionBar item={item} onAsk={handleAskFromBar} />

      {/* Context modal */}
      {contextModalOpen && item.context && (
        <ContextModal context={item.context} onClose={() => setContextModalOpen(false)} />
      )}
    </div>
  );
}
