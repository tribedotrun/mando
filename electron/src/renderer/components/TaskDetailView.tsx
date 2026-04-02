import React, { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useMountEffect } from '#renderer/hooks/useMountEffect';
import { fetchTimeline, fetchItemSessions, fetchTranscript, fetchPrSummary } from '#renderer/api';
import type { TaskItem, SessionEntry, SessionSummary, TimelineEvent } from '#renderer/types';
import { FINALIZED_STATUSES } from '#renderer/types';
import { fmtDuration, shortRepo, prLabel, prHref } from '#renderer/utils';
import { StatusIcon } from '#renderer/components/TaskActions';
import { TranscriptSidebar } from '#renderer/components/TranscriptViewer';
import { SessionDetailPanel } from '#renderer/components/SessionDetailPanel';
import { PrSections } from '#renderer/components/PrSections';
import { TaskActionBar } from '#renderer/components/TaskActionBar';
import { TaskTimeline } from '#renderer/components/TaskTimeline';

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
  const [transcriptSession, setTranscriptSession] = useState<{
    entry: SessionEntry;
    markdown: string | null;
    loading: boolean;
  } | null>(null);
  const [transcriptFullScreen, setTranscriptFullScreen] = useState(false);

  // Refs to avoid stale closure in mount-only keydown handler
  const transcriptFullScreenRef = React.useRef(transcriptFullScreen);
  transcriptFullScreenRef.current = transcriptFullScreen;
  const transcriptSessionRef = React.useRef(transcriptSession);
  transcriptSessionRef.current = transcriptSession;
  const onBackRef = React.useRef(onBack);
  onBackRef.current = onBack;

  // Escape key: close full-screen transcript, then close sidebar, then go back.
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
      } else if (transcriptSessionRef.current) {
        setTranscriptSession(null);
      } else {
        onBackRef.current();
      }
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  });

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
    } catch {
      setTranscriptSession((p) =>
        p?.entry.session_id === sessionId ? { ...p, markdown: null, loading: false } : p,
      );
    }
  };

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="shrink-0 border-b pb-4" style={{ borderColor: 'var(--color-border-subtle)' }}>
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
              className="text-[18px] font-semibold leading-snug"
              style={{ color: 'var(--color-text-1)' }}
            >
              {item.title}
              {item.linear_id && (
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
              )}
            </h1>
            <div className="mt-2 flex flex-wrap items-center gap-2">
              <StatusIcon status={item.status} />
              {item.project && (
                <span className="text-[11px]" style={{ color: 'var(--color-text-2)' }}>
                  {shortRepo(item.project)}
                </span>
              )}
              {item.pr && item.project && (
                <a
                  href={prHref(item.pr, item.project)}
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
            {onMerge && item.pr && item.status === 'awaiting-review' && (
              <ActionButton label="Merge" onClick={onMerge} accent />
            )}
            {onReopen && FINALIZED_STATUSES.includes(item.status) && (
              <ActionButton label="Reopen" onClick={onReopen} />
            )}
            {onRework && FINALIZED_STATUSES.includes(item.status) && (
              <ActionButton label="Rework" onClick={onRework} />
            )}
            {(item.branch || item.worktree || item.plan) && <DetailOverflowMenu item={item} />}
          </div>
        </div>
      </div>

      {/* Body: two columns */}
      <div className="flex min-h-0 flex-1 gap-0">
        {/* Left: details + bottom action bar */}
        <div className="flex min-h-0 flex-1 flex-col">
          <div className="min-h-0 flex-1 overflow-auto pr-4 pt-4">
            {/* Original prompt */}
            {item.original_prompt && (
              <DetailSection label="Request">
                <p className="text-[13px] italic" style={{ color: 'var(--color-text-2)' }}>
                  {item.original_prompt}
                </p>
              </DetailSection>
            )}

            {/* Context — hidden by default */}
            {item.context && <ContextToggle context={item.context} />}

            {/* Metadata — no_pr delivery note shown inline, rest behind overflow */}
            {item.no_pr && (
              <div
                className="mb-3 text-[12px]"
                style={{ color: 'var(--color-accent)', fontWeight: 500 }}
              >
                Findings only — no PR
              </div>
            )}

            {/* PR Description — sectioned */}
            {prBody?.summary && (
              <DetailSection label="PR">
                <PrSections text={prBody.summary} />
              </DetailSection>
            )}

            {/* Escalation / Error */}
            {item.escalation_report && (
              <DetailSection label="Escalation Report">
                <pre
                  className="whitespace-pre-wrap rounded p-3 text-[11px]"
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

            {/* Timeline */}
            <DetailSection label={`Timeline (${events.length})`}>
              <TaskTimeline events={events} onTranscriptClick={handleTranscriptClick} />
            </DetailSection>
          </div>

          {/* Bottom action bar — pinned below scroll area */}
          <TaskActionBar item={item} />
        </div>

        {/* Right: transcript sidebar */}
        {transcriptSession && !transcriptFullScreen && (
          <TranscriptSidebar
            session={transcriptSession}
            onClose={() => setTranscriptSession(null)}
            onExpand={() => setTranscriptFullScreen(true)}
          />
        )}
      </div>

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
    </div>
  );
}

// ── Sub-components ──

function ActionButton({
  label,
  onClick,
  accent,
}: {
  label: string;
  onClick: () => void;
  accent?: boolean;
}): React.ReactElement {
  return (
    <button
      onClick={onClick}
      className="rounded-md px-4 py-1.5 text-[13px] font-medium"
      style={{
        background: accent ? 'var(--color-accent)' : 'transparent',
        color: accent ? 'var(--color-bg)' : 'var(--color-text-2)',
        border: accent ? 'none' : '1px solid var(--color-border)',
        cursor: 'pointer',
      }}
    >
      {label}
    </button>
  );
}

function DetailSection({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <div className="mb-5">
      <div
        className="mb-2 text-[10px] font-medium uppercase tracking-widest"
        style={{ color: 'var(--color-text-4)' }}
      >
        {label}
      </div>
      {children}
    </div>
  );
}

function DetailOverflowMenu({ item }: { item: TaskItem }): React.ReactElement {
  const [open, setOpen] = useState(false);

  const copyAndClose = (text: string) => {
    navigator.clipboard.writeText(text).catch(() => {});
    setOpen(false);
  };

  const entries: { label: string; value: string }[] = [];
  if (item.branch) entries.push({ label: 'Copy branch', value: item.branch });
  if (item.worktree) entries.push({ label: 'Copy working directory', value: item.worktree });
  if (item.plan) {
    const planLabel = item.plan.endsWith('adopt-handoff.md')
      ? 'Copy handoff path'
      : 'Copy brief path';
    entries.push({ label: planLabel, value: item.plan });
  }

  return (
    <div
      className="relative"
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget)) setOpen(false);
      }}
    >
      <button
        onClick={() => setOpen((v) => !v)}
        aria-label="More info"
        className="flex items-center justify-center rounded"
        style={{
          width: 28,
          height: 28,
          background: 'transparent',
          color: 'var(--color-text-2)',
          border: '1px solid var(--color-border)',
          cursor: 'pointer',
          fontSize: 14,
          borderRadius: 6,
        }}
      >
        &hellip;
      </button>
      {open && (
        <div
          className="absolute right-0 top-full z-50 mt-1 min-w-[220px] rounded-lg py-1"
          style={{
            background: 'var(--color-surface-3)',
            border: '1px solid var(--color-border)',
            boxShadow: '0 4px 16px rgba(0,0,0,0.3)',
          }}
        >
          {entries.map(({ label, value }) => (
            <button
              key={label}
              onClick={() => copyAndClose(value)}
              className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-[12px] hover:bg-[var(--color-surface-2)]"
              style={{
                color: 'var(--color-text-1)',
                background: 'none',
                border: 'none',
                cursor: 'pointer',
              }}
            >
              <svg
                width="12"
                height="12"
                viewBox="0 0 12 12"
                fill="none"
                stroke="var(--color-text-3)"
                strokeWidth="1.2"
                strokeLinecap="round"
              >
                <rect x="4" y="4" width="7" height="7" rx="1" />
                <path d="M8 4V2.5A1.5 1.5 0 006.5 1H2.5A1.5 1.5 0 001 2.5v4A1.5 1.5 0 002.5 8H4" />
              </svg>
              {label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function ContextToggle({ context }: { context: string }): React.ReactElement {
  const [open, setOpen] = useState(false);
  return (
    <div className="mb-5">
      <button
        onClick={() => setOpen((v) => !v)}
        className="mb-2 flex items-center gap-1.5 text-[10px] font-medium uppercase tracking-widest"
        style={{
          color: 'var(--color-text-4)',
          background: 'none',
          border: 'none',
          cursor: 'pointer',
          padding: 0,
        }}
      >
        <svg
          width="8"
          height="8"
          viewBox="0 0 8 8"
          fill="currentColor"
          style={{
            transition: 'transform 150ms',
            transform: open ? 'rotate(90deg)' : 'none',
          }}
        >
          <path d="M2 1l4 3-4 3V1z" />
        </svg>
        Context
      </button>
      {open && (
        <p className="text-[11px] leading-relaxed" style={{ color: 'var(--color-text-3)' }}>
          {context}
        </p>
      )}
    </div>
  );
}
