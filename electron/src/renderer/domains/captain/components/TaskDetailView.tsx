import React, { useCallback, useMemo, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import log from '#renderer/logger';
import { toast } from 'sonner';
import {
  fetchTimeline,
  fetchItemSessions,
  fetchTranscript,
  fetchPrSummary,
  fetchAskHistory,
} from '#renderer/domains/captain/hooks/useApi';
import {
  FINALIZED_STATUSES,
  type TaskItem,
  type SessionEntry,
  type SessionSummary,
  type TimelineEvent,
  type MandoConfig,
} from '#renderer/types';
import { prLabel, prHref, extractClarifierQuestions } from '#renderer/utils';
import {
  TranscriptSidebar,
  SessionDetailPanel,
  buildSessionsFromTimeline,
} from '#renderer/domains/sessions';
import { TaskActionBar } from '#renderer/domains/captain/components/TaskActionBar';
import { StatusCard } from '#renderer/domains/captain/components/StatusCard';
import {
  ActiveQAView,
  QAHistoryTab,
  type QAHandle,
} from '#renderer/domains/captain/components/TaskQAView';
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

type DetailTab = 'timeline' | 'pr' | 'sessions' | 'info' | 'qa' | 'terminal';
const REFRESH_INDICATOR_MS = 1500;

interface Props {
  item: TaskItem;
  onBack: () => void;
  onMerge?: () => void;
  onOpenTerminal?: (opts: { project: string; cwd: string; resumeSessionId?: string }) => void;
}

export function TaskDetailView({
  item,
  onBack,
  onMerge,
  onOpenTerminal,
}: Props): React.ReactElement {
  const qc = useQueryClient();
  const [activeTab, setActiveTab] = useState<DetailTab>('pr');
  const [prRefreshing, setPrRefreshing] = useState(false);
  const [transcriptSession, setTranscriptSession] = useState<{
    entry: SessionEntry;
    markdown: string | null;
    loading: boolean;
  } | null>(null);
  const [transcriptFullScreen, setTranscriptFullScreen] = useState(false);
  const [activeQA, setActiveQA] = useState(false);
  const [contextModalOpen, setContextModalOpen] = useState(false);

  // Listen for header overflow menu triggering "View task brief"
  useMountEffect(() => {
    const handler = () => setContextModalOpen(true);
    document.addEventListener('mando:view-task-brief', handler);
    return () => document.removeEventListener('mando:view-task-brief', handler);
  });

  const qaRef = useRef<QAHandle | null>(null);
  const [pendingQuestion, setPendingQuestion] = useState<string | null>(null);
  const sidebarRef = useRef<HTMLDivElement>(null);

  // Refs to avoid stale closures in keydown handler.
  const transcriptFullScreenRef = useRef(transcriptFullScreen);
  transcriptFullScreenRef.current = transcriptFullScreen;
  const transcriptSessionRef = useRef(transcriptSession);
  transcriptSessionRef.current = transcriptSession;
  const activeQARef = useRef(activeQA);
  activeQARef.current = activeQA;
  const onBackRef = useRef(onBack);
  onBackRef.current = onBack;

  // Escape key handler, layered dismiss.
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
      if (transcriptFullScreenRef.current) {
        setTranscriptFullScreen(false);
      } else if (activeQARef.current) {
        setActiveQA(false);
      } else if (transcriptSessionRef.current) {
        setTranscriptSession(null);
      } else {
        onBackRef.current();
      }
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  });

  // Click-outside to close transcript sidebar.
  useMountEffect(() => {
    const onMouseDown = (e: MouseEvent) => {
      if (!transcriptSessionRef.current || transcriptFullScreenRef.current) return;
      if (
        document.querySelector('[role="dialog"]') ||
        document.querySelector('[data-command-palette]') ||
        document.querySelector('[data-shortcut-overlay]')
      )
        return;
      if (sidebarRef.current?.contains(e.target as Node)) return;
      setTranscriptSession(null);
    };
    document.addEventListener('mousedown', onMouseDown);
    return () => document.removeEventListener('mousedown', onMouseDown);
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

  const handleTranscriptClick = async (sessionId: string, event: TimelineEvent) => {
    const summary = sessionMap[sessionId];
    const stub: SessionEntry = {
      session_id: sessionId,
      created_at: summary?.started_at || event.timestamp,
      cwd: summary?.cwd || item.worktree || '',
      model: '',
      caller: summary?.caller || 'worker',
      resumed: summary?.resumed ? 1 : 0,
      task_id: String(item.id),
      worker_name: summary?.worker_name || '',
      status: summary?.status || '',
    };
    setTranscriptSession({ entry: stub, markdown: null, loading: true });
    try {
      const data = await fetchTranscript(sessionId);
      setTranscriptSession((p) =>
        p?.entry.session_id === sessionId ? { ...p, markdown: data.markdown, loading: false } : p,
      );
    } catch (err) {
      log.warn('Failed to fetch transcript for session', sessionId, err);
      toast.error('Transcript unavailable, check daemon logs');
      setTranscriptSession((p) =>
        p?.entry.session_id === sessionId ? { ...p, markdown: null, loading: false } : p,
      );
    }
  };

  const handleSessionClick = (s: SessionSummary) => {
    const stub: SessionEntry = {
      session_id: s.session_id,
      created_at: s.started_at || '',
      cwd: s.cwd || item.worktree || '',
      model: s.model || '',
      caller: s.caller || 'worker',
      resumed: s.resumed ? 1 : 0,
      task_id: String(item.id),
      worker_name: s.worker_name || '',
      status: s.status || '',
    };
    setTranscriptSession({ entry: stub, markdown: null, loading: true });
    fetchTranscript(s.session_id)
      .then((data) => {
        setTranscriptSession((p) =>
          p?.entry.session_id === s.session_id
            ? { ...p, markdown: data.markdown, loading: false }
            : p,
        );
      })
      .catch((err) => {
        log.warn('Failed to fetch transcript:', err);
        toast.error('Transcript unavailable, check daemon logs');
        setTranscriptSession((p) =>
          p?.entry.session_id === s.session_id ? { ...p, markdown: null, loading: false } : p,
        );
      });
  };

  const handleAskFromBar = useCallback((question: string) => {
    if (activeQARef.current && qaRef.current) {
      qaRef.current.ask(question);
    } else {
      setActiveQA(true);
      setTranscriptSession(null);
      setPendingQuestion(question);
    }
  }, []);

  const handleQABack = useCallback(() => {
    setActiveQA(false);
  }, []);

  const openTerminalPage = useCallback(
    (resumeId?: string) => {
      if (!onOpenTerminal || !item.project) return;
      let cwd = item.worktree;
      if (!cwd) {
        const cfg = qc.getQueryData<MandoConfig>(queryKeys.config.current());
        const pc = cfg?.captain?.projects
          ? Object.values(cfg.captain.projects).find((p) => p.name === item.project)
          : undefined;
        cwd = pc?.path;
      }
      if (!cwd) {
        toast.error(`No working directory for task "${item.title}"`);
        return;
      }
      onOpenTerminal({
        project: item.project,
        cwd,
        resumeSessionId: resumeId,
      });
    },
    [onOpenTerminal, item.project, item.worktree, item.title, qc],
  );

  const handleResumeSession = useCallback(
    (sessionId: string) => openTerminalPage(sessionId),
    [openTerminalPage],
  );

  const tabs: { key: DetailTab; label: string }[] = [
    { key: 'pr', label: 'PR' },
    { key: 'timeline', label: 'Timeline' },
    { key: 'sessions', label: 'Sessions' },
    { key: 'info', label: 'Info' },
    { key: 'qa', label: 'Q&A' },
    { key: 'terminal', label: 'Terminal' },
  ];

  const showMerge = onMerge && item.pr_number && item.project && item.status === 'awaiting-review';

  return (
    <div className="flex h-full flex-col">
      {/* Main row */}
      <div className="flex min-h-0 flex-1">
        {/* Left column, entire column scrolls together */}
        <div className="min-h-0 min-w-0 flex-1 overflow-y-auto overflow-x-hidden">
          {/* Header */}
          <div className="pb-3">
            {/* Title row and actions inline */}
            <div className="mb-1 flex items-start gap-3">
              <div className="min-w-0 flex-1">
                <h1 className="break-words text-heading font-semibold leading-snug text-foreground">
                  {item.title}
                </h1>
                {/* Metadata */}
                <div className="mt-0.5 flex flex-wrap items-center gap-2">
                  {item.pr_number && (item.github_repo || item.project) && (
                    <a
                      href={prHref(item.pr_number, (item.github_repo ?? item.project)!)}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-caption text-muted-foreground no-underline hover:underline"
                    >
                      {prLabel(item.pr_number)}
                    </a>
                  )}
                  {item.no_pr && (
                    <span className="text-caption text-muted-foreground">Findings only</span>
                  )}
                </div>
              </div>
              {showMerge && (
                <div className="flex shrink-0 items-center gap-2">
                  <Button onClick={onMerge} size="sm">
                    Merge
                  </Button>
                </div>
              )}
            </div>

            {/* Status card */}
            <div className="mt-3 ml-6">
              <StatusCard item={item} sessions={sessions} clarifierQuestions={clarifierQuestions} />
            </div>
          </div>

          {/* Active Q&A mode, replaces tabs */}
          {activeQA ? (
            <div className="flex min-h-0 flex-1 flex-col">
              <ActiveQAView
                item={item}
                qaRef={qaRef}
                onBack={handleQABack}
                pendingQuestion={pendingQuestion}
                onPendingConsumed={() => setPendingQuestion(null)}
              />
            </div>
          ) : (
            <Tabs
              value={activeTab}
              onValueChange={(v) => {
                if (v === 'terminal') {
                  openTerminalPage();
                } else {
                  setActiveTab(v as DetailTab);
                }
              }}
              className="ml-6 gap-0"
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
                {activeTab === 'pr' && item.pr_number && (
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
                {activeTab === 'timeline' && (
                  <TimelineTab
                    events={events}
                    onTranscriptClick={(...args) => void handleTranscriptClick(...args)}
                  />
                )}
                {activeTab === 'pr' && <PrTab item={item} prBody={prBody} prPending={prPending} />}
                {activeTab === 'sessions' && (
                  <SessionsTab
                    sessions={sessions}
                    onSessionClick={handleSessionClick}
                    onResumeSession={handleResumeSession}
                    taskId={item.id}
                  />
                )}
                {activeTab === 'info' && <InfoTab item={item} />}
                {activeTab === 'qa' && <QAHistoryTab item={item} />}
              </div>
            </Tabs>
          )}
        </div>

        {/* Transcript sidebar */}
        {transcriptSession && !transcriptFullScreen && (
          <div ref={sidebarRef} className="flex h-full shrink-0">
            <TranscriptSidebar
              session={transcriptSession}
              onClose={() => setTranscriptSession(null)}
              onExpand={() => setTranscriptFullScreen(true)}
            />
          </div>
        )}
      </div>

      {/* Action bar, pinned at bottom */}
      <TaskActionBar item={item} onAsk={handleAskFromBar} />

      {/* Full-screen transcript overlay */}
      {transcriptSession && transcriptFullScreen && (
        <div className="fixed inset-0 z-[300] bg-background">
          <div className="h-full p-6">
            <SessionDetailPanel
              session={transcriptSession.entry}
              markdown={transcriptSession.markdown}
              loading={transcriptSession.loading}
              error={null}
              onClose={() => setTranscriptFullScreen(false)}
              resumeCmd={
                transcriptSession.entry.cwd
                  ? `cd ${transcriptSession.entry.cwd} && claude --resume ${transcriptSession.entry.session_id}`
                  : `claude --resume ${transcriptSession.entry.session_id}`
              }
            />
          </div>
        </div>
      )}

      {/* Context modal */}
      {contextModalOpen && item.context && (
        <ContextModal context={item.context} onClose={() => setContextModalOpen(false)} />
      )}
    </div>
  );
}
