import React, { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useMountEffect } from '#renderer/hooks/useMountEffect';
import { fetchTimeline, fetchItemSessions, fetchTranscript, fetchPrSummary } from '#renderer/api';
import type { TaskItem, SessionEntry, SessionSummary, TimelineEvent } from '#renderer/types';
import { fmtDuration, shortRepo, prLabel, prHref } from '#renderer/utils';
import { StatusIcon } from '#renderer/components/TaskActions';
import { TranscriptSidebar } from '#renderer/components/TranscriptViewer';
import { SessionDetailPanel } from '#renderer/components/SessionDetailPanel';
import { PrMarkdown } from '#renderer/components/PrMarkdown';
import { TaskTimeline } from '#renderer/components/TaskTimeline';

interface Props {
  item: TaskItem;
  onBack: () => void;
  onMerge?: () => void;
  onReopen?: () => void;
  onRework?: () => void;
  onAnswer?: (answer: string) => void;
  onReopenWithFeedback?: (feedback: string) => void;
  onReworkWithFeedback?: (feedback: string) => void;
}

export function TaskDetailView({
  item,
  onBack,
  onMerge,
  onReopen,
  onRework,
  onAnswer,
  onReopenWithFeedback,
  onReworkWithFeedback,
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
            {onReopen && <ActionButton label="Reopen" onClick={onReopen} />}
            {onRework && <ActionButton label="Rework" onClick={onRework} />}
            {(item.branch || item.worktree || item.plan) && <DetailOverflowMenu item={item} />}
          </div>
        </div>
      </div>

      {/* Body: two columns */}
      <div className="flex min-h-0 flex-1 gap-0">
        {/* Left: details + timeline */}
        <div className="min-h-0 flex-1 overflow-auto pr-4 pt-4">
          {/* Inline feedback for action-needed statuses */}
          {(item.status === 'awaiting-review' ||
            item.status === 'escalated' ||
            item.status === 'needs-clarification') && (
            <InlineFeedbackInput
              item={item}
              onReopen={onReopenWithFeedback}
              onRework={onReworkWithFeedback}
              onAnswer={onAnswer}
            />
          )}

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

          {/* PR Summary — sectioned */}
          {prBody?.summary && <PrSections body={prBody.summary} />}

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

// ── PR section parsing ──

interface PrSection {
  heading: string;
  content: string;
  level: number;
}

/** Classify a heading into a bucket: 'overview', 'evidence', or 'details'. */
function classifySection(heading: string): 'overview' | 'evidence' | 'details' {
  const h = heading.toLowerCase();
  if (
    /summary|overview|description|problem|what|why|changes|background|tl;?dr/.test(h) &&
    !/diagram/.test(h)
  ) {
    return 'overview';
  }
  if (
    /evidence|screenshot|verification|before.*after|after|demo|visual|test.*plan|test.*result/.test(
      h,
    )
  ) {
    return 'evidence';
  }
  // PR Summary with diagram goes to overview
  if (/pr summary|summary diagram/.test(h)) {
    return 'overview';
  }
  return 'details';
}

/** Split PR body by top-level (##) headings into sections. */
function parsePrSections(body: string): PrSection[] {
  const lines = body.split('\n');
  const sections: PrSection[] = [];
  let currentHeading = '';
  let currentLevel = 0;
  let currentLines: string[] = [];

  for (const line of lines) {
    const headingMatch = line.match(/^(#{1,4})\s+(.*)/);
    if (headingMatch) {
      // Flush previous section
      if (currentHeading || currentLines.length > 0) {
        sections.push({
          heading: currentHeading,
          content: currentLines.join('\n').trim(),
          level: currentLevel,
        });
      }
      currentHeading = headingMatch[2].trim();
      currentLevel = headingMatch[1].length;
      currentLines = [];
    } else {
      currentLines.push(line);
    }
  }
  // Flush last section
  if (currentHeading || currentLines.length > 0) {
    sections.push({
      heading: currentHeading,
      content: currentLines.join('\n').trim(),
      level: currentLevel,
    });
  }
  return sections;
}

function PrSections({ body }: { body: string }): React.ReactElement {
  const sections = parsePrSections(body);

  // Group into buckets
  const overview: PrSection[] = [];
  const evidence: PrSection[] = [];
  const details: PrSection[] = [];

  for (const s of sections) {
    if (!s.heading) {
      // Preamble text before any heading — treat as overview
      if (s.content) overview.push(s);
      continue;
    }
    const bucket = classifySection(s.heading);
    if (bucket === 'overview') overview.push(s);
    else if (bucket === 'evidence') evidence.push(s);
    else details.push(s);
  }

  return (
    <>
      {/* Overview / Problem Statement */}
      {overview.length > 0 && (
        <DetailSection label="Overview">
          {overview.map((s, i) => (
            <div key={i}>
              {s.heading && (
                <div
                  className="mt-2 mb-1 text-[12px] font-semibold"
                  style={{ color: 'var(--color-text-1)' }}
                >
                  {s.heading}
                </div>
              )}
              {s.content && <PrMarkdown text={s.content} />}
            </div>
          ))}
        </DetailSection>
      )}

      {/* Evidence / Verification */}
      {evidence.length > 0 && (
        <DetailSection label="Verification">
          {evidence.map((s, i) => (
            <div key={i}>
              {s.heading && (
                <div
                  className="mt-2 mb-1 text-[12px] font-semibold"
                  style={{ color: 'var(--color-text-1)' }}
                >
                  {s.heading}
                </div>
              )}
              {s.content && <PrMarkdown text={s.content} />}
            </div>
          ))}
        </DetailSection>
      )}

      {/* Details (collapsed by default) */}
      {details.length > 0 && <PrDetailsToggle sections={details} />}
    </>
  );
}

function PrDetailsToggle({ sections }: { sections: PrSection[] }): React.ReactElement {
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
        Details ({sections.length})
      </button>
      {open &&
        sections.map((s, i) => (
          <div key={i}>
            {s.heading && (
              <div
                className="mt-2 mb-1 text-[12px] font-semibold"
                style={{ color: 'var(--color-text-1)' }}
              >
                {s.heading}
              </div>
            )}
            {s.content && <PrMarkdown text={s.content} />}
          </div>
        ))}
    </div>
  );
}

// ── Inline feedback input ──

function InlineFeedbackInput({
  item,
  onReopen,
  onRework,
  onAnswer,
}: {
  item: TaskItem;
  onReopen?: (feedback: string) => void;
  onRework?: (feedback: string) => void;
  onAnswer?: (answer: string) => void;
}): React.ReactElement {
  const [text, setText] = useState('');

  const isNeedsClarification = item.status === 'needs-clarification';
  const isAwaitingReview = item.status === 'awaiting-review';

  const placeholder = isNeedsClarification
    ? 'Provide the answer or clarification...'
    : 'Provide feedback or instructions...';

  const handleSubmit = (action: 'reopen' | 'rework' | 'answer') => {
    const value = text.trim();
    if (!value) return;
    if (action === 'answer' && onAnswer) {
      onAnswer(value);
    } else if (action === 'reopen' && onReopen) {
      onReopen(value);
    } else if (action === 'rework' && onRework) {
      onRework(value);
    }
    setText('');
  };

  const statusLabel = isNeedsClarification
    ? 'Needs your input'
    : isAwaitingReview
      ? 'Ready for your review'
      : 'Escalated — needs attention';

  const statusColor = isNeedsClarification
    ? 'var(--color-needs-human)'
    : isAwaitingReview
      ? 'var(--color-success)'
      : 'var(--color-error)';

  return (
    <div
      className="mb-5 rounded-lg p-3"
      style={{
        background: `color-mix(in srgb, ${statusColor} 6%, transparent)`,
        border: `1px solid color-mix(in srgb, ${statusColor} 20%, transparent)`,
      }}
    >
      <div className="mb-2 text-[11px] font-medium" style={{ color: statusColor }}>
        {statusLabel}
      </div>
      <textarea
        className="mb-2 w-full rounded-md px-3 py-2 text-[13px] focus:outline-none"
        style={{
          background: 'var(--color-surface-1)',
          color: 'var(--color-text-1)',
          border: '1px solid var(--color-border-subtle)',
          resize: 'vertical',
        }}
        rows={2}
        placeholder={placeholder}
        value={text}
        onChange={(e) => setText(e.target.value)}
      />
      <div className="flex gap-2">
        {isNeedsClarification && onAnswer && (
          <button
            onClick={() => handleSubmit('answer')}
            disabled={!text.trim()}
            className="rounded-md px-3 py-1 text-[12px] font-medium disabled:opacity-40"
            style={{
              background: 'var(--color-accent)',
              color: 'var(--color-bg)',
              border: 'none',
              cursor: text.trim() ? 'pointer' : 'default',
            }}
          >
            Answer
          </button>
        )}
        {!isNeedsClarification && onReopen && (
          <button
            onClick={() => handleSubmit('reopen')}
            disabled={!text.trim()}
            className="rounded-md px-3 py-1 text-[12px] font-medium disabled:opacity-40"
            style={{
              background: 'var(--color-accent)',
              color: 'var(--color-bg)',
              border: 'none',
              cursor: text.trim() ? 'pointer' : 'default',
            }}
          >
            Reopen with feedback
          </button>
        )}
        {!isNeedsClarification && onRework && (
          <button
            onClick={() => handleSubmit('rework')}
            disabled={!text.trim()}
            className="rounded-md px-3 py-1 text-[12px] font-medium disabled:opacity-40"
            style={{
              background: 'transparent',
              color: 'var(--color-text-2)',
              border: '1px solid var(--color-border)',
              cursor: text.trim() ? 'pointer' : 'default',
            }}
          >
            Rework
          </button>
        )}
      </div>
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
