import React, { useState } from 'react';
import type { TaskItem, SessionSummary, TimelineEvent } from '#renderer/types';
import { copyToClipboard, fmtDuration, fmtUsd, relativeTime, shortenPath } from '#renderer/utils';
import { PrSections } from '#renderer/domains/captain/components/PrSections';
import { PrMarkdown } from '#renderer/domains/captain/components/PrMarkdown';
import { TaskTimeline } from '#renderer/domains/captain/components/TaskTimeline';
import { formatCallerLabel, buildSessionSequence, SessionDot } from '#renderer/domains/sessions';
import { useFocusTrap } from '#renderer/global/hooks/useFocusTrap';

/* ── Timeline tab ── */

export function TimelineTab({
  events,
  onTranscriptClick,
}: {
  events: TimelineEvent[];
  onTranscriptClick: (sessionId: string, event: TimelineEvent) => void;
}): React.ReactElement {
  const reversed = [...events].reverse();
  return <TaskTimeline events={reversed} onTranscriptClick={onTranscriptClick} />;
}

/* ── PR tab ── */

export function PrTab({
  item,
  prBody,
  prPending,
  onRefresh,
}: {
  item: TaskItem;
  prBody: { summary: string | null } | undefined;
  prPending: boolean;
  onRefresh?: () => void;
}): React.ReactElement {
  const [refreshing, setRefreshing] = useState(false);

  const handleRefresh = () => {
    if (!onRefresh || refreshing) return;
    setRefreshing(true);
    onRefresh();
    setTimeout(() => setRefreshing(false), 1500);
  };

  if (!item.pr) {
    return (
      <div className="text-caption" style={{ color: 'var(--color-text-3)' }}>
        No PR associated with this task
      </div>
    );
  }
  if (prPending && !prBody) {
    return (
      <div
        className="flex items-center gap-2 text-caption"
        style={{ color: 'var(--color-text-3)', minHeight: '120px' }}
      >
        <span
          className="inline-block h-3 w-3 animate-spin rounded-full"
          style={{ border: '1.5px solid var(--color-text-4)', borderTopColor: 'transparent' }}
        />
        Loading PR info...
      </div>
    );
  }
  if (!prBody?.summary) {
    return (
      <div className="text-caption italic" style={{ color: 'var(--color-text-3)' }}>
        No PR description available
      </div>
    );
  }
  return (
    <PrSections
      text={prBody.summary}
      onRefresh={onRefresh ? handleRefresh : undefined}
      refreshing={refreshing}
    />
  );
}

/* ── Sessions tab ── */

export function SessionsTab({
  sessions,
  onSessionClick,
  taskId,
}: {
  sessions: SessionSummary[];
  onSessionClick: (s: SessionSummary) => void;
  taskId: number;
}): React.ReactElement {
  if (sessions.length === 0) {
    return (
      <div className="text-caption" style={{ color: 'var(--color-text-3)' }}>
        No sessions yet
      </div>
    );
  }

  const totalCost = sessions.reduce((s, x) => s + (x.cost_usd ?? 0), 0);
  const totalDuration = sessions.reduce((s, x) => s + (x.duration_ms ?? 0), 0);

  const reversed = [...sessions].reverse();

  // buildSessionSequence expects newest-first input (it reverses internally).
  const seqMap = buildSessionSequence(
    reversed.map((s) => ({
      session_id: s.session_id,
      created_at: s.started_at || '',
      cwd: s.cwd || '',
      model: s.model || '',
      caller: s.caller,
      resumed: s.resumed ? 1 : 0,
      task_id: String(taskId),
      worker_name: s.worker_name || '',
      status: s.status,
    })),
  );

  return (
    <div>
      {reversed.map((s) => {
        const label =
          formatCallerLabel(s.caller).charAt(0).toUpperCase() +
          formatCallerLabel(s.caller).slice(1);
        const seq = seqMap.get(s.session_id);
        const title = seq ? `${label} #${seq}` : label;

        return (
          <button
            key={s.session_id}
            onClick={() => onSessionClick(s)}
            className="mb-1 flex w-full items-center gap-2 rounded-md px-2 py-2 text-left hover:bg-[var(--color-surface-2)]"
            style={{ background: 'none', border: 'none', cursor: 'pointer' }}
          >
            <SessionDot status={s.status} />
            <div className="min-w-0 flex-1">
              <div className="text-body font-medium" style={{ color: 'var(--color-text-1)' }}>
                {title}
                {s.worker_name ? ` (${s.worker_name})` : ''}
              </div>
              <div className="text-caption" style={{ color: 'var(--color-text-3)' }}>
                {s.started_at && <span>{relativeTime(s.started_at)}</span>}
                {s.model && <span> &middot; {s.model}</span>}
                {s.duration_ms != null && s.duration_ms > 0 && (
                  <span> &middot; {fmtDuration(s.duration_ms / 1000)}</span>
                )}
                {s.cost_usd != null && s.cost_usd > 0 && (
                  <span> &middot; ${fmtUsd(s.cost_usd)}</span>
                )}
              </div>
            </div>
            <span className="text-caption" style={{ color: 'var(--color-text-4)' }}>
              {s.status}
            </span>
          </button>
        );
      })}

      <div
        className="mt-2 flex items-center gap-2 border-t pt-3 text-caption"
        style={{ borderColor: 'var(--color-border-subtle)', color: 'var(--color-text-3)' }}
      >
        {sessions.length} sessions
        {totalDuration > 0 && <span>&middot; {fmtDuration(totalDuration / 1000)}</span>}
        {totalCost > 0 && <span>&middot; ${fmtUsd(totalCost)}</span>}
      </div>
    </div>
  );
}

/* ── Info tab ── */

export function InfoTab({ item }: { item: TaskItem }): React.ReactElement {
  const [contextExpanded, setContextExpanded] = useState(false);
  const [escalationExpanded, setEscalationExpanded] = useState(false);

  return (
    <div className="space-y-4">
      {/* ── Details grid ── */}
      <div
        className="grid gap-x-4 gap-y-2 rounded-lg px-4 py-3"
        style={{
          background: 'var(--color-surface-2)',
          gridTemplateColumns: 'auto 1fr',
          alignItems: 'baseline',
        }}
      >
        <span className="text-caption" style={{ color: 'var(--color-text-4)' }}>
          ID
        </span>
        <span className="text-body" style={{ color: 'var(--color-text-2)' }}>
          #{item.id}
        </span>

        {item.branch && (
          <>
            <span className="text-caption" style={{ color: 'var(--color-text-4)' }}>
              Branch
            </span>
            <CopyValue value={item.branch} />
          </>
        )}

        {item.worktree && (
          <>
            <span className="text-caption" style={{ color: 'var(--color-text-4)' }}>
              Worktree
            </span>
            <CopyValue value={item.worktree} display={shortenPath(item.worktree)} />
          </>
        )}

        {item.plan && (
          <>
            <span className="text-caption" style={{ color: 'var(--color-text-4)' }}>
              Plan
            </span>
            <CopyValue value={item.plan} display={shortenPath(item.plan)} />
          </>
        )}
      </div>

      {/* ── Content group ── */}
      {item.original_prompt && (
        <InfoSection label="Original Request">
          <p className="text-body leading-relaxed" style={{ color: 'var(--color-text-2)' }}>
            {item.original_prompt}
          </p>
        </InfoSection>
      )}

      {/* Escalation report, collapsed by default */}
      {item.escalation_report && (
        <CollapsibleSection
          label="Escalation Report"
          expanded={escalationExpanded}
          onToggle={() => setEscalationExpanded((v) => !v)}
        >
          <pre
            className="whitespace-pre-wrap break-words rounded-md p-3 text-code"
            style={{
              background: 'var(--color-surface-2)',
              color: 'var(--color-text-1)',
              border: '1px solid color-mix(in srgb, var(--color-error) 30%, transparent)',
            }}
          >
            {item.escalation_report}
          </pre>
        </CollapsibleSection>
      )}

      {/* Task brief, collapsed by default */}
      {item.context && (
        <CollapsibleSection
          label="Task Brief"
          expanded={contextExpanded}
          onToggle={() => setContextExpanded((v) => !v)}
        >
          <PrMarkdown text={item.context} />
        </CollapsibleSection>
      )}
    </div>
  );
}

function InfoSection({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <div>
      <div className="mb-2 text-label" style={{ color: 'var(--color-text-4)' }}>
        {label}
      </div>
      {children}
    </div>
  );
}

function CollapsibleSection({
  label,
  expanded,
  onToggle,
  children,
}: {
  label: string;
  expanded: boolean;
  onToggle: () => void;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <div>
      <button
        onClick={onToggle}
        className="mb-2 flex items-center gap-2 text-label"
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
            transform: expanded ? 'rotate(90deg)' : 'none',
          }}
        >
          <path d="M2 1l4 3-4 3V1z" />
        </svg>
        {label}
      </button>
      {expanded && children}
    </div>
  );
}

function CopyValue({ value, display }: { value: string; display?: string }): React.ReactElement {
  const [copied, setCopied] = useState(false);
  return (
    <button
      onClick={async () => {
        const ok = await copyToClipboard(value);
        if (ok) {
          setCopied(true);
          setTimeout(() => setCopied(false), 1200);
        }
      }}
      className="inline-flex items-center gap-2 text-left text-code hover:opacity-80"
      style={{
        color: 'var(--color-text-2)',
        background: 'none',
        border: 'none',
        cursor: 'pointer',
        padding: 0,
      }}
    >
      <span className="min-w-0 break-all">{display ?? value}</span>
      {copied ? (
        <svg
          width="12"
          height="12"
          viewBox="0 0 12 12"
          fill="none"
          stroke="var(--color-success)"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d="M2.5 6.5l2.5 2.5 4.5-5" />
        </svg>
      ) : (
        <svg
          width="12"
          height="12"
          viewBox="0 0 12 12"
          fill="none"
          stroke="var(--color-text-4)"
          strokeWidth="1.2"
          strokeLinecap="round"
        >
          <rect x="4" y="4" width="7" height="7" rx="1" />
          <path d="M8 4V2.5A1.5 1.5 0 006.5 1H2.5A1.5 1.5 0 001 2.5v4A1.5 1.5 0 002.5 8H4" />
        </svg>
      )}
    </button>
  );
}

/* ── Context modal ── */

export function ContextModal({
  context,
  onClose,
}: {
  context: string;
  onClose: () => void;
}): React.ReactElement {
  const { ref: dialogRef, handleKeyDown } = useFocusTrap(onClose);

  return (
    <div
      data-testid="context-modal"
      role="dialog"
      aria-modal="true"
      aria-label="Context"
      className="fixed inset-0 z-[200] flex items-center justify-center bg-black/60"
      onClick={(e) => e.target === e.currentTarget && onClose()}
      onKeyDown={handleKeyDown}
    >
      <div
        ref={dialogRef}
        className="flex w-[560px] max-w-[90vw] flex-col rounded-lg"
        style={{
          background: 'var(--color-surface-2)',
          border: '1px solid var(--color-border)',
          maxHeight: '70vh',
        }}
      >
        <div className="flex shrink-0 items-center justify-between px-5 pt-4 pb-3">
          <h3 className="text-subheading" style={{ color: 'var(--color-text-1)' }}>
            Context
          </h3>
          <button
            onClick={onClose}
            className="flex items-center justify-center rounded"
            style={{
              width: 24,
              height: 24,
              background: 'none',
              border: 'none',
              color: 'var(--color-text-3)',
              cursor: 'pointer',
            }}
          >
            <svg
              width="14"
              height="14"
              viewBox="0 0 14 14"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
            >
              <path d="M3 3l8 8M11 3l-8 8" />
            </svg>
          </button>
        </div>
        <div className="overflow-y-auto px-5 pb-5">
          <PrMarkdown text={context} />
        </div>
      </div>
    </div>
  );
}
