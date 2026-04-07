import React, { useState } from 'react';
import { Check, ChevronRight, Copy, X } from 'lucide-react';
import type { TaskItem, SessionSummary, TimelineEvent } from '#renderer/types';
import { copyToClipboard, fmtDuration, relativeTime, shortenPath } from '#renderer/utils';
import { PrSections } from '#renderer/domains/captain/components/PrSections';
import { PrMarkdown } from '#renderer/domains/captain/components/PrMarkdown';
import { TaskTimeline } from '#renderer/domains/captain/components/TaskTimeline';
import { formatCallerLabel, buildSessionSequence, SessionDot } from '#renderer/domains/sessions';
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogClose,
} from '#renderer/global/components/Dialog';
import { Spinner } from '#renderer/global/components/Spinner';

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
    return <div className="text-caption text-text-3">No PR associated with this task</div>;
  }
  if (prPending && !prBody) {
    return (
      <div
        className="flex items-center gap-2 text-caption text-text-3"
        style={{ minHeight: '120px' }}
      >
        <Spinner size={12} color="var(--color-text-4)" borderWidth={1.5} />
        Loading PR info...
      </div>
    );
  }
  if (!prBody?.summary) {
    return <div className="text-caption italic text-text-3">No PR description available</div>;
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
    return <div className="text-caption text-text-3">No sessions yet</div>;
  }

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
            className="mb-1 flex w-full items-center gap-2 rounded-md px-2 py-2 text-left hover:bg-surface-2"
            style={{ background: 'none', border: 'none', cursor: 'pointer' }}
          >
            <SessionDot status={s.status} />
            <div className="min-w-0 flex-1">
              <div className="text-body font-medium text-text-1">
                {title}
                {s.worker_name ? ` (${s.worker_name})` : ''}
              </div>
              <div className="text-caption text-text-3">
                {s.started_at && <span>{relativeTime(s.started_at)}</span>}
                {s.model && <span> &middot; {s.model}</span>}
                {s.duration_ms != null && s.duration_ms > 0 && (
                  <span> &middot; {fmtDuration(s.duration_ms / 1000)}</span>
                )}
              </div>
            </div>
            <span className="text-caption text-text-4">{s.status}</span>
          </button>
        );
      })}

      <div
        className="mt-2 flex items-center gap-2 border-t pt-3 text-caption text-text-3"
        style={{ borderColor: 'var(--color-border-subtle)' }}
      >
        {sessions.length} sessions
        {totalDuration > 0 && <span>&middot; {fmtDuration(totalDuration / 1000)}</span>}
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
        className="grid gap-x-4 gap-y-2 rounded-lg bg-surface-2 px-4 py-3"
        style={{
          gridTemplateColumns: 'auto 1fr',
          alignItems: 'baseline',
        }}
      >
        <span className="text-caption text-text-4">ID</span>
        <span className="text-body text-text-2">#{item.id}</span>

        {item.branch && (
          <>
            <span className="text-caption text-text-4">Branch</span>
            <CopyValue value={item.branch} />
          </>
        )}

        {item.worktree && (
          <>
            <span className="text-caption text-text-4">Worktree</span>
            <CopyValue value={item.worktree} display={shortenPath(item.worktree)} />
          </>
        )}

        {item.plan && (
          <>
            <span className="text-caption text-text-4">Plan</span>
            <CopyValue value={item.plan} display={shortenPath(item.plan)} />
          </>
        )}
      </div>

      {/* ── Content group ── */}
      {item.original_prompt && (
        <InfoSection label="Original Request">
          <p className="text-body leading-relaxed text-text-2">{item.original_prompt}</p>
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
            className="whitespace-pre-wrap break-words rounded-md bg-surface-2 p-3 text-code text-text-1"
            style={{
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
      <div className="mb-2 text-label text-text-4">{label}</div>
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
        className="mb-2 flex items-center gap-2 text-label text-text-4"
        style={{
          background: 'none',
          border: 'none',
          cursor: 'pointer',
          padding: 0,
        }}
      >
        <ChevronRight
          size={8}
          style={{
            transition: 'transform 150ms',
            transform: expanded ? 'rotate(90deg)' : 'none',
          }}
        />
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
      className="inline-flex items-center gap-2 text-left text-code text-text-2 hover:opacity-80"
      style={{
        background: 'none',
        border: 'none',
        cursor: 'pointer',
        padding: 0,
      }}
    >
      <span className="min-w-0 break-all">{display ?? value}</span>
      {copied ? (
        <Check size={12} color="var(--color-success)" />
      ) : (
        <Copy size={12} color="var(--color-text-4)" />
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
  return (
    <Dialog
      open={true}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent
        data-testid="context-modal"
        className="flex w-[560px] max-w-[90vw] flex-col"
        style={{ maxHeight: '70vh', padding: 0 }}
      >
        <div className="flex shrink-0 items-center justify-between px-5 pt-4 pb-3">
          <DialogTitle className="mb-0">Context</DialogTitle>
          <DialogClose
            className="flex items-center justify-center rounded text-text-3 cursor-pointer"
            style={{
              width: 24,
              height: 24,
              background: 'none',
              border: 'none',
            }}
          >
            <X size={14} />
          </DialogClose>
        </div>
        <div className="overflow-y-auto px-5 pb-5">
          <PrMarkdown text={context} />
        </div>
      </DialogContent>
    </Dialog>
  );
}
