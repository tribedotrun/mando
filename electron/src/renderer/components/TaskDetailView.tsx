import React, { useCallback, useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useMountEffect } from '#renderer/hooks/useMountEffect';
import log from '#renderer/logger';
import {
  fetchTimeline,
  fetchItemSessions,
  fetchTranscript,
  fetchPrSummary,
  fetchAskHistory,
} from '#renderer/api';
import type {
  TaskItem,
  SessionEntry,
  SessionSummary,
  TimelineEvent,
  ClarifierQuestion,
} from '#renderer/types';
import { FINALIZED_STATUSES } from '#renderer/types';
import { shortRepo, prLabel, prHref } from '#renderer/utils';
import { TranscriptSidebar } from '#renderer/components/TranscriptViewer';
import { SessionDetailPanel } from '#renderer/components/SessionDetailPanel';
import { TaskActionBar } from '#renderer/components/TaskActionBar';
import { StatusCard } from '#renderer/components/StatusCard';
import { DetailOverflowMenu } from '#renderer/components/TaskDetailParts';
import { ActiveQAView, QAHistoryTab, type QAHandle } from '#renderer/components/TaskQAView';
import {
  TimelineTab,
  PrTab,
  SessionsTab,
  InfoTab,
  ContextModal,
} from '#renderer/components/TaskDetailTabs';

type DetailTab = 'timeline' | 'pr' | 'sessions' | 'info' | 'qa';

interface Props {
  item: TaskItem;
  onBack: () => void;
  onMerge?: () => void;
}

export function TaskDetailView({ item, onBack, onMerge }: Props): React.ReactElement {
  const [activeTab, setActiveTab] = useState<DetailTab>('pr');
  const [transcriptSession, setTranscriptSession] = useState<{
    entry: SessionEntry;
    markdown: string | null;
    loading: boolean;
  } | null>(null);
  const [transcriptFullScreen, setTranscriptFullScreen] = useState(false);
  const [activeQA, setActiveQA] = useState(false);
  const [contextModalOpen, setContextModalOpen] = useState(false);
  const qaRef = useRef<QAHandle | null>(null);
  const [pendingQuestion, setPendingQuestion] = useState<string | null>(null);

  // Refs to avoid stale closures in keydown handler.
  const transcriptFullScreenRef = useRef(transcriptFullScreen);
  transcriptFullScreenRef.current = transcriptFullScreen;
  const transcriptSessionRef = useRef(transcriptSession);
  transcriptSessionRef.current = transcriptSession;
  const activeQARef = useRef(activeQA);
  activeQARef.current = activeQA;
  const onBackRef = useRef(onBack);
  onBackRef.current = onBack;

  // Escape key handler — layered dismiss.
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

  // Data queries.
  const { data: timelineData } = useQuery({
    queryKey: ['task-detail-timeline', item.id],
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
    queryKey: ['task-detail-pr', item.id],
    queryFn: async () => {
      const data = await fetchPrSummary(item.id);
      // Persist to disk for finalized tasks.
      if (isFinalized && data.summary) {
        localStorage.setItem(`pr-cache:${item.id}`, JSON.stringify(data));
      }
      return data;
    },
    enabled: !!item.pr,
    staleTime: isFinalized ? Infinity : 30_000,
    initialData: () => {
      if (!isFinalized) return undefined;
      const cached = localStorage.getItem(`pr-cache:${item.id}`);
      return cached ? JSON.parse(cached) : undefined;
    },
  });

  const { data: qaHistory } = useQuery({
    queryKey: ['task-ask-history', item.id],
    queryFn: () => fetchAskHistory(item.id),
  });
  const hasQAHistory = (qaHistory?.history?.length ?? 0) > 0 || activeQA;

  const events = timelineData?.events ?? [];
  const sessionMap = timelineData?.sessionMap ?? {};

  // Timeline is the authoritative source for session data.
  const sessions = buildSessionsFromTimeline(events, sessionMap, item);

  // Extract clarifier questions from latest timeline event.
  const clarifierQuestions: ClarifierQuestion[] | null = (() => {
    if (item.status !== 'needs-clarification') return null;
    for (let i = events.length - 1; i >= 0; i--) {
      const e = events[i];
      if (e.event_type !== 'clarify_question') continue;
      const q = e.data?.questions;
      if (Array.isArray(q)) return q as ClarifierQuestion[];
    }
    return null;
  })();

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
      setPendingQuestion(question);
    }
  }, []);

  const handleQABack = useCallback(() => {
    setActiveQA(false);
  }, []);

  // Tab definitions.
  const tabs: { key: DetailTab; label: string; badge?: boolean }[] = [
    { key: 'pr', label: 'PR' },
    { key: 'timeline', label: 'Timeline' },
    { key: 'sessions', label: 'Sessions' },
    { key: 'info', label: 'Info' },
  ];
  if (hasQAHistory) {
    tabs.push({ key: 'qa', label: 'Q&A' });
  }

  const showMerge = onMerge && item.pr && item.project && item.status === 'awaiting-review';

  return (
    <div className="flex h-full flex-col">
      {/* Main row */}
      <div className="flex min-h-0 flex-1">
        {/* Left column — entire column scrolls together */}
        <div className="min-h-0 min-w-0 flex-1 overflow-y-auto overflow-x-hidden">
          {/* Header */}
          <div className="pb-3">
            {/* Title row — back, title, and actions inline */}
            <div className="mb-1 flex items-start gap-3">
              <button
                onClick={onBack}
                className="mt-1.5 shrink-0 text-caption"
                style={{
                  color: 'var(--color-text-3)',
                  background: 'none',
                  border: 'none',
                  cursor: 'pointer',
                }}
              >
                &larr;
              </button>
              <div className="min-w-0 flex-1">
                <h1
                  className="break-words text-heading font-semibold leading-snug"
                  style={{ color: 'var(--color-text-1)' }}
                >
                  {item.title}
                </h1>
                {/* Metadata */}
                <div className="mt-0.5 flex flex-wrap items-center gap-2">
                  {item.project && (
                    <span className="text-caption" style={{ color: 'var(--color-text-3)' }}>
                      {shortRepo(item.project)}
                    </span>
                  )}
                  {item.pr && (item.github_repo || item.project) && (
                    <a
                      href={prHref(item.pr, (item.github_repo ?? item.project)!)}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-caption no-underline hover:underline"
                      style={{ color: 'var(--color-accent)' }}
                    >
                      {prLabel(item.pr)}
                    </a>
                  )}
                  {item.no_pr && (
                    <span className="text-caption" style={{ color: 'var(--color-accent)' }}>
                      Findings only
                    </span>
                  )}
                </div>
              </div>
              <div className="flex shrink-0 items-center gap-2">
                {showMerge && (
                  <button
                    onClick={onMerge}
                    className="rounded-md px-4 py-1.5 text-caption font-semibold"
                    style={{
                      background: 'var(--color-accent)',
                      color: 'var(--color-bg)',
                      border: 'none',
                      cursor: 'pointer',
                    }}
                  >
                    Merge
                  </button>
                )}
                {(item.branch || item.worktree || item.plan || item.context) && (
                  <DetailOverflowMenu item={item} onViewContext={() => setContextModalOpen(true)} />
                )}
              </div>
            </div>

            {/* Status card */}
            <div className="mt-3 ml-6">
              <StatusCard item={item} sessions={sessions} clarifierQuestions={clarifierQuestions} />
            </div>
          </div>

          {/* Active Q&A mode — replaces tabs */}
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
            <>
              {/* Tabs — sticky within scroll */}
              <div
                className="sticky top-0 z-10 ml-6 flex gap-0"
                style={{
                  borderBottom: '1px solid var(--color-border-subtle)',
                  background: 'var(--color-bg)',
                }}
              >
                {tabs.map((tab) => {
                  const isActive = tab.key === activeTab;
                  return (
                    <button
                      key={tab.key}
                      onClick={() => setActiveTab(tab.key)}
                      className="rounded-t px-3 py-1.5 text-caption font-medium transition-colors hover:bg-[var(--color-surface-2)]"
                      style={{
                        color: isActive ? 'var(--color-text-1)' : 'var(--color-text-3)',
                        background: 'none',
                        border: 'none',
                        borderBottom: isActive
                          ? '2px solid var(--color-accent)'
                          : '2px solid transparent',
                        cursor: 'pointer',
                        marginBottom: -1,
                      }}
                    >
                      {tab.label}
                    </button>
                  );
                })}
              </div>

              {/* Tab content */}
              <div className="ml-6 break-words pt-4 pr-2">
                {activeTab === 'timeline' && (
                  <TimelineTab events={events} onTranscriptClick={handleTranscriptClick} />
                )}
                {activeTab === 'pr' && (
                  <PrTab
                    item={item}
                    prBody={prBody}
                    prPending={prPending}
                    onRefresh={() => refetchPr()}
                  />
                )}
                {activeTab === 'sessions' && (
                  <SessionsTab
                    sessions={sessions}
                    onSessionClick={handleSessionClick}
                    taskId={item.id}
                  />
                )}
                {activeTab === 'info' && <InfoTab item={item} />}
                {activeTab === 'qa' && <QAHistoryTab item={item} />}
              </div>
            </>
          )}
        </div>

        {/* Transcript sidebar */}
        {transcriptSession && !transcriptFullScreen && (
          <TranscriptSidebar
            session={transcriptSession}
            onClose={() => setTranscriptSession(null)}
            onExpand={() => setTranscriptFullScreen(true)}
          />
        )}
      </div>

      {/* Action bar — pinned at bottom */}
      <TaskActionBar item={item} onAsk={handleAskFromBar} />

      {/* Full-screen transcript overlay */}
      {transcriptSession && transcriptFullScreen && (
        <div className="fixed inset-0 z-[300]" style={{ background: 'var(--color-bg)' }}>
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

/* ── Recent event pill ── */

/* ── Sessions fallback from timeline events ── */

const CALLER_MAP: Record<string, string> = {
  worker_spawned: 'worker',
  worker_completed: 'worker',
  worker_nudged: 'worker',
  session_resumed: 'worker',
  captain_review_started: 'captain-review-async',
  captain_review_verdict: 'captain-review-async',
  clarify_started: 'clarifier',
  clarify_resolved: 'clarifier',
  clarify_question: 'clarifier',
  human_ask: 'task-ask',
};

function buildSessionsFromTimeline(
  events: TimelineEvent[],
  sessionMap: Record<string, SessionSummary>,
  item: TaskItem,
): SessionSummary[] {
  const seen = new Map<string, SessionSummary>();
  for (const ev of events) {
    const sid = ev.data?.session_id as string | undefined;
    if (!sid || seen.has(sid)) continue;
    const existing = sessionMap[sid];
    seen.set(sid, {
      session_id: sid,
      status: existing?.status ?? 'stopped',
      caller: existing?.caller ?? CALLER_MAP[ev.event_type] ?? 'worker',
      started_at: existing?.started_at ?? ev.timestamp,
      duration_ms: existing?.duration_ms,
      cost_usd: existing?.cost_usd,
      model: existing?.model,
      resumed: existing?.resumed ?? false,
      cwd: existing?.cwd ?? item.worktree,
      worker_name: existing?.worker_name,
    });
  }
  return [...seen.values()];
}
