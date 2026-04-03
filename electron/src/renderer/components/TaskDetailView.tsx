import React, { useCallback, useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useMountEffect } from '#renderer/hooks/useMountEffect';
import { fetchTimeline, fetchItemSessions, fetchTranscript, fetchPrSummary } from '#renderer/api';
import type {
  TaskItem,
  SessionEntry,
  SessionSummary,
  TimelineEvent,
  ClarifierQuestion,
} from '#renderer/types';
import { FINALIZED_STATUSES } from '#renderer/types';
import { fmtDuration, shortRepo, prLabel, prHref, linearHref } from '#renderer/utils';
import { useLinearSlug } from '#renderer/hooks/useLinearSlug';
import { StatusIcon } from '#renderer/components/TaskActions';
import { TranscriptSidebar } from '#renderer/components/TranscriptViewer';
import { SessionDetailPanel } from '#renderer/components/SessionDetailPanel';
import { PrSections } from '#renderer/components/PrSections';
import { TaskActionBar } from '#renderer/components/TaskActionBar';
import { TaskTimeline } from '#renderer/components/TaskTimeline';
import { ClarificationSection } from '#renderer/components/ClarificationSection';
import { TaskQA, type TaskQAHandle } from '#renderer/components/TaskQA';
import { TaskQAExpanded } from '#renderer/components/TaskQAExpanded';
import {
  ActionButton,
  DetailSection,
  DetailOverflowMenu,
  ContextToggle,
} from '#renderer/components/TaskDetailParts';

interface Props {
  item: TaskItem;
  onBack: () => void;
  onMerge?: () => void;
  onReopen?: () => void;
  onRework?: () => void;
}

export function TaskDetailView({
  item,
  onBack,
  onMerge,
  onReopen,
  onRework,
}: Props): React.ReactElement {
  const linearSlug = useLinearSlug();
  const [transcriptSession, setTranscriptSession] = useState<{
    entry: SessionEntry;
    markdown: string | null;
    loading: boolean;
  } | null>(null);
  const [transcriptFullScreen, setTranscriptFullScreen] = useState(false);
  const [qaExpanded, setQaExpanded] = useState(false);
  const [qaOpen, setQaOpen] = useState(true);
  const qaRef = useRef<TaskQAHandle | null>(null);

  // Refs to avoid stale closure in mount-only keydown handler
  const transcriptFullScreenRef = React.useRef(transcriptFullScreen);
  transcriptFullScreenRef.current = transcriptFullScreen;
  const transcriptSessionRef = React.useRef(transcriptSession);
  transcriptSessionRef.current = transcriptSession;
  const qaExpandedRef = React.useRef(qaExpanded);
  qaExpandedRef.current = qaExpanded;
  const onBackRef = React.useRef(onBack);
  onBackRef.current = onBack;

  // Escape key: close full-screen transcript, then Q&A expanded, then sidebar, then go back.
  // Skip if another overlay (modal, command palette) is open.
  useMountEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      // Don't handle if a modal, command palette, or shortcut overlay is open.
      if (
        document.querySelector('[role="dialog"]') ||
        document.querySelector('[data-command-palette]') ||
        document.querySelector('[data-shortcut-overlay]')
      )
        return;
      e.stopPropagation();
      if (transcriptFullScreenRef.current) {
        setTranscriptFullScreen(false);
      } else if (qaExpandedRef.current) {
        setQaExpanded(false);
      } else if (transcriptSessionRef.current) {
        setTranscriptSession(null);
      } else {
        onBackRef.current();
      }
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  });

  const handleAskFromBar = useCallback((question: string) => {
    setQaOpen(true);
    // Small delay so the Q&A component mounts before receiving the ask.
    setTimeout(() => qaRef.current?.askFromBar(question), 50);
  }, []);

  const handleQaClose = useCallback(() => {
    setQaOpen(false);
  }, []);

  const handleQaExpand = useCallback(() => {
    setQaExpanded(true);
  }, []);

  const { data: timelineData } = useQuery({
    queryKey: ['task-detail-timeline', item.id],
    queryFn: async () => {
      const [tl, sess] = await Promise.all([fetchTimeline(item.id), fetchItemSessions(item.id)]);
      const map: Record<string, SessionSummary> = {};
      for (const s of sess.sessions) map[s.session_id] = s;
      return { events: tl.events, sessionMap: map, sessions: sess.sessions };
    },
  });

  const { data: prBody } = useQuery({
    queryKey: ['task-detail-pr', item.id],
    queryFn: () => fetchPrSummary(item.id),
    enabled: !!item.pr,
  });

  const events = timelineData?.events ?? [];
  const sessionMap = timelineData?.sessionMap ?? {};
  const sessions = timelineData?.sessions ?? [];

  const totalDuration = sessions.reduce((sum, s) => sum + (s.duration_ms ?? 0), 0);

  // Extract structured questions from the latest clarify_question timeline event.
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
      console.warn('Failed to fetch transcript for session', sessionId, err);
      setTranscriptSession((p) =>
        p?.entry.session_id === sessionId ? { ...p, markdown: null, loading: false } : p,
      );
    }
  };

  return (
    <div className="flex h-full flex-col">
      {/* Main row: left column (header + content) + sidebar */}
      <div className="flex min-h-0 flex-1">
        {/* Left column */}
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          {/* Header */}
          <div
            className="shrink-0 border-b pb-4"
            style={{ borderColor: 'var(--color-border-subtle)' }}
          >
            <button
              onClick={onBack}
              className="mb-3 flex items-center gap-1.5 text-[12px]"
              style={{
                color: 'var(--color-text-3)',
                background: 'none',
                border: 'none',
                cursor: 'pointer',
              }}
            >
              &larr; Back to list
            </button>
            <div className="flex items-start gap-3">
              <div className="min-w-0 flex-1">
                <h1
                  className="break-words text-[18px] font-semibold leading-snug"
                  style={{ color: 'var(--color-text-1)' }}
                >
                  {item.title}
                  {item.linear_id &&
                    (linearSlug ? (
                      <a
                        href={linearHref(item.linear_id, linearSlug)}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="ml-2 inline-flex items-center align-middle font-mono no-underline hover:underline"
                        style={{
                          fontSize: 11,
                          fontWeight: 400,
                          color: 'var(--color-text-3)',
                          background: 'var(--color-surface-3)',
                          padding: '2px 6px',
                          borderRadius: 3,
                        }}
                      >
                        {item.linear_id}
                      </a>
                    ) : (
                      <span
                        className="ml-2 inline-flex items-center align-middle font-mono"
                        style={{
                          fontSize: 11,
                          fontWeight: 400,
                          color: 'var(--color-text-3)',
                          background: 'var(--color-surface-3)',
                          padding: '2px 6px',
                          borderRadius: 3,
                        }}
                      >
                        {item.linear_id}
                      </span>
                    ))}
                </h1>
                <div className="mt-2 flex flex-wrap items-center gap-2">
                  <StatusIcon status={item.status} />
                  {item.project && (
                    <span className="text-[11px]" style={{ color: 'var(--color-text-2)' }}>
                      {shortRepo(item.project)}
                    </span>
                  )}
                  {item.pr && (item.github_repo || item.project) && (
                    <a
                      href={prHref(item.pr, (item.github_repo ?? item.project)!)}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-[11px] no-underline hover:underline"
                      style={{ color: 'var(--color-accent)' }}
                    >
                      {prLabel(item.pr)}
                    </a>
                  )}
                  {sessions.length > 0 && (
                    <span className="text-[11px]" style={{ color: 'var(--color-text-3)' }}>
                      {sessions.length} sessions &middot; {fmtDuration(totalDuration / 1000)}
                    </span>
                  )}
                </div>
              </div>
              <div className="flex shrink-0 items-center gap-2">
                {onReopen && FINALIZED_STATUSES.includes(item.status) && (
                  <ActionButton label="Reopen" onClick={onReopen} />
                )}
                {onRework && FINALIZED_STATUSES.includes(item.status) && (
                  <ActionButton label="Rework" onClick={onRework} />
                )}
                {onMerge && item.pr && item.project && item.status === 'awaiting-review' && (
                  <ActionButton label="Merge" onClick={onMerge} accent />
                )}
                {(item.branch || item.worktree || item.plan) && <DetailOverflowMenu item={item} />}
              </div>
            </div>
          </div>

          {/* Scrollable details */}
          <div className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden break-words pr-4 pt-4">
            {item.original_prompt && (
              <DetailSection label="Request">
                <p className="text-[13px] italic" style={{ color: 'var(--color-text-2)' }}>
                  {item.original_prompt}
                </p>
              </DetailSection>
            )}

            {/* Clarification questions — prominent when needs-clarification */}
            {clarifierQuestions && clarifierQuestions.length > 0 && (
              <ClarificationSection key={item.id} taskId={item.id} questions={clarifierQuestions} />
            )}

            {item.context && <ContextToggle context={item.context} />}
            {item.no_pr && (
              <div
                className="mb-3 text-[12px]"
                style={{ color: 'var(--color-accent)', fontWeight: 500 }}
              >
                Findings only — no PR
              </div>
            )}
            {prBody?.summary && (
              <DetailSection label="PR">
                <PrSections text={prBody.summary} />
              </DetailSection>
            )}
            {item.escalation_report && (
              <DetailSection label="Escalation Report">
                <pre
                  className="whitespace-pre-wrap break-words rounded p-3 text-[11px]"
                  style={{
                    background: 'var(--color-surface-2)',
                    color: 'var(--color-text-1)',
                    border: '1px solid color-mix(in srgb, var(--color-error) 30%, transparent)',
                  }}
                >
                  {item.escalation_report}
                </pre>
              </DetailSection>
            )}
            <DetailSection label={`Timeline (${events.length})`}>
              <TaskTimeline events={events} onTranscriptClick={handleTranscriptClick} />
            </DetailSection>
          </div>
        </div>

        {/* Sidebar — spans full height from top to bottom */}
        {transcriptSession && !transcriptFullScreen && (
          <TranscriptSidebar
            session={transcriptSession}
            onClose={() => setTranscriptSession(null)}
            onExpand={() => setTranscriptFullScreen(true)}
          />
        )}
      </div>

      {/* Q&A section — between content and action bar */}
      {qaOpen && (
        <TaskQA
          key={item.id}
          item={item}
          qaRef={qaRef}
          onExpand={handleQaExpand}
          onClose={handleQaClose}
        />
      )}

      {/* Action bar — pinned at bottom, full width */}
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
              onClose={() => {
                setTranscriptFullScreen(false);
              }}
              resumeCmd={
                transcriptSession.entry.cwd
                  ? `cd ${transcriptSession.entry.cwd} && claude --resume ${transcriptSession.entry.session_id}`
                  : `claude --resume ${transcriptSession.entry.session_id}`
              }
            />
          </div>
        </div>
      )}

      {/* Expanded Q&A overlay */}
      {qaExpanded && (
        <TaskQAExpanded
          item={item}
          initialMessages={qaRef.current?.getMessages()}
          onBack={() => setQaExpanded(false)}
          onClose={() => {
            setQaExpanded(false);
            setQaOpen(false);
          }}
        />
      )}
    </div>
  );
}
