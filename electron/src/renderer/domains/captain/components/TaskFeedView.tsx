import React, { useCallback, useMemo, useRef, useState } from 'react';
import { buildUrl } from '#renderer/global/hooks/useApi';
import { useTaskFeed } from '#renderer/hooks/queries';
import { useTaskAdvisor } from '#renderer/hooks/mutations';
import { PrMarkdown } from '#renderer/domains/captain/components/PrMarkdown';
import { MessageBlock } from '#renderer/domains/captain/components/MessageBlock';
import type {
  TaskItem,
  FeedItem,
  TimelineEvent,
  TaskArtifact,
  AskHistoryEntry,
  ClarifierQuestion,
} from '#renderer/types';
import {
  MessageSquare,
  FileText,
  Image,
  ChevronDown,
  ChevronRight,
  ArrowUp,
  AlertTriangle,
  Clock,
  Loader2,
} from 'lucide-react';
import { cn } from '#renderer/cn';
import { canReopen, canRework, clamp } from '#renderer/utils';
import { StatusIcon } from '#renderer/global/components/StatusIndicator';
import { ClarificationTab } from '#renderer/domains/captain/components/StatusCard';

// ── Timeline event icon mapping ──
const EVENT_ICON_MAP: Record<string, string> = {
  created: 'queued',
  worker_spawned: 'in-progress',
  worker_completed: 'awaiting-review',
  captain_review_started: 'captain-reviewing',
  captain_review_verdict: 'captain-reviewing',
  captain_merge_started: 'captain-merging',
  merged: 'merged',
  escalated: 'escalated',
  errored: 'errored',
  canceled: 'canceled',
  human_reopen: 'queued',
  human_ask: 'awaiting-review',
  rework_requested: 'rework',
  evidence_updated: 'awaiting-review',
  work_summary_updated: 'awaiting-review',
};

// ── Feed Block Components ──

function firstLine(s: string, max: number): string {
  const line = s.split('\n').find((l) => l.trim().length > 0) ?? s;
  const trimmed = line.trim();
  return trimmed.length > max ? `${trimmed.slice(0, max).trimEnd()}…` : trimmed;
}

function TimelineBlock({ event }: { event: TimelineEvent }) {
  const iconStatus = EVENT_ICON_MAP[event.event_type] ?? 'queued';
  const time = new Date(event.timestamp).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });
  // Prefer the classifier's short reason, but fall back to the first line of
  // the nudge content so events predating the reason field still show context.
  const nudgeReason =
    event.event_type === 'worker_nudged'
      ? ((event.data?.reason as string | null | undefined) ??
        ((event.data?.content as string | null | undefined)
          ? firstLine(event.data.content as string, 140)
          : null))
      : null;

  return (
    <div className="flex items-start gap-3 px-4 py-2">
      <div className="mt-0.5 flex-shrink-0">
        <StatusIcon status={iconStatus} />
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <span className="text-caption text-text-2">{time}</span>
          <span className="text-caption font-medium text-text-2">{event.actor}</span>
        </div>
        <p className="text-body-sm text-text-1">{event.summary}</p>
        {nudgeReason ? (
          <p className="mt-0.5 text-caption text-text-3">Reason: {nudgeReason}</p>
        ) : null}
      </div>
    </div>
  );
}

function EvidenceBlock({ artifact }: { artifact: TaskArtifact }) {
  const [expanded, setExpanded] = useState(false);
  const time = new Date(artifact.created_at).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });
  const mediaCount = artifact.media?.length ?? 0;

  return (
    <div className="mx-4 my-2 rounded-lg border border-border bg-surface-1 p-4">
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center gap-3 text-left"
      >
        <Image size={16} className="flex-shrink-0 text-accent" />
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline gap-2">
            <span className="text-body-sm font-medium text-text-1">Evidence</span>
            <span className="text-caption text-text-3">{mediaCount} file(s)</span>
            <span className="text-caption text-text-3">{time}</span>
          </div>
        </div>
        {expanded ? (
          <ChevronDown size={14} className="text-text-3" />
        ) : (
          <ChevronRight size={14} className="text-text-3" />
        )}
      </button>
      {expanded && (
        <div className="mt-3 space-y-3">
          {artifact.media?.map((m) => {
            const isImage = ['png', 'jpg', 'jpeg', 'gif', 'webp'].includes(m.ext);
            const mediaUrl = buildUrl(`/api/artifacts/${artifact.id}/media/${m.index}`);
            return (
              <div key={m.index} className="space-y-1">
                {isImage && m.local_path && (
                  <img
                    src={mediaUrl}
                    alt={m.caption ?? m.filename}
                    className="max-h-64 rounded border border-border object-contain"
                  />
                )}
                <div className="flex items-center gap-2 text-caption text-text-3">
                  <FileText size={12} />
                  <span>{m.caption ?? m.filename}</span>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

function WorkSummaryBlock({ artifact }: { artifact: TaskArtifact }) {
  const [expanded, setExpanded] = useState(true);
  const time = new Date(artifact.created_at).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });

  return (
    <div className="mx-4 my-2 rounded-lg border border-border bg-surface-1 p-4">
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center gap-3 text-left"
      >
        <FileText size={16} className="flex-shrink-0 text-accent" />
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline gap-2">
            <span className="text-body-sm font-medium text-text-1">Work Summary</span>
            <span className="text-caption text-text-3">{time}</span>
          </div>
        </div>
        {expanded ? (
          <ChevronDown size={14} className="text-text-3" />
        ) : (
          <ChevronRight size={14} className="text-text-3" />
        )}
      </button>
      {expanded && (
        <div className="mt-3">
          <pre className="max-h-80 overflow-auto whitespace-pre-wrap rounded bg-surface-2 p-3 font-mono text-caption text-text-2">
            {artifact.content}
          </pre>
        </div>
      )}
    </div>
  );
}

function EscalationBlock({ event, report }: { event: TimelineEvent; report?: string | null }) {
  const time = new Date(event.timestamp).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });

  return (
    <div
      className="mx-4 my-2 rounded-lg px-4 py-3"
      style={{
        background: 'color-mix(in srgb, var(--destructive) 6%, transparent)',
        border: '1px solid color-mix(in srgb, var(--destructive) 20%, transparent)',
      }}
    >
      <div className="mb-2 flex items-center gap-2">
        <AlertTriangle size={14} className="text-destructive" />
        <span className="text-body-sm font-medium text-destructive">Escalated</span>
        <span className="text-caption text-text-3">{time}</span>
      </div>
      {report ? (
        <div className="text-body-sm text-text-1">
          <PrMarkdown text={report} />
        </div>
      ) : (
        <p className="text-body-sm text-text-1">{event.summary}</p>
      )}
    </div>
  );
}

function ClarificationBlock({
  event,
  taskId,
  isActive,
}: {
  event: TimelineEvent;
  taskId: number;
  isActive: boolean;
}) {
  const time = new Date(event.timestamp).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });
  const questions = (event.data?.questions as ClarifierQuestion[]) ?? [];

  if (isActive && questions.length > 0) {
    return (
      <div className="mx-4 my-2">
        <ClarificationTab taskId={taskId} questions={questions} />
      </div>
    );
  }

  return (
    <div
      className="mx-4 my-2 rounded-lg px-4 py-3"
      style={{
        background: 'color-mix(in srgb, var(--needs-human) 6%, transparent)',
        border: '1px solid color-mix(in srgb, var(--needs-human) 20%, transparent)',
      }}
    >
      <div className="mb-2 flex items-center gap-2">
        <MessageSquare size={14} style={{ color: 'var(--needs-human)' }} />
        <span className="text-body-sm font-medium" style={{ color: 'var(--needs-human)' }}>
          Clarification requested
        </span>
        <span className="text-caption text-text-3">{time}</span>
      </div>
      {questions.map((q, i) => (
        <div key={i} className="mb-1 text-body-sm text-text-2">
          <span className="text-text-3">{i + 1}.</span> {q.question}
          {q.self_answered && (
            <span className="ml-1 text-caption text-text-3">(auto-resolved)</span>
          )}
        </div>
      ))}
    </div>
  );
}

function FeedBlock({
  item,
  task,
  isLatestClarify,
}: {
  item: FeedItem;
  task: TaskItem;
  isLatestClarify: (timestamp: string) => boolean;
}) {
  switch (item.type) {
    case 'timeline': {
      const event = item.data as TimelineEvent;
      if (event.event_type === 'escalated') {
        return <EscalationBlock event={event} report={task.escalation_report} />;
      }
      if (event.event_type === 'clarify_question') {
        const active = task.status === 'needs-clarification' && isLatestClarify(event.timestamp);
        return <ClarificationBlock event={event} taskId={task.id} isActive={active} />;
      }
      // Suppress events that have a richer renderer elsewhere in the feed:
      //   work_summary_updated / evidence_updated → artifact cards
      //   human_ask → the "You" MessageBlock already shows the question
      if (
        event.event_type === 'work_summary_updated' ||
        event.event_type === 'evidence_updated' ||
        event.event_type === 'human_ask'
      ) {
        return null;
      }
      return <TimelineBlock event={event} />;
    }
    case 'artifact': {
      const artifact = item.data as TaskArtifact;
      if (artifact.artifact_type === 'evidence') return <EvidenceBlock artifact={artifact} />;
      if (artifact.artifact_type === 'work_summary')
        return <WorkSummaryBlock artifact={artifact} />;
      return null;
    }
    case 'message':
      return <MessageBlock entry={item.data as AskHistoryEntry} />;
    default:
      return null;
  }
}

// ── Advisor Input Bar ──

function AdvisorInputBar({
  item,
  onSend,
  isPending,
}: {
  item: TaskItem;
  onSend: (message: string, intent: string) => void;
  isPending: boolean;
}) {
  const [input, setInput] = useState('');
  const [intent, setIntent] = useState<'ask' | 'reopen' | 'rework'>('ask');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const handleSubmit = useCallback(() => {
    const trimmed = input.trim();
    if (!trimmed || isPending) return;
    onSend(trimmed, intent);
    setInput('');
    setIntent('ask');
  }, [input, isPending, onSend, intent]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        handleSubmit();
      }
    },
    [handleSubmit],
  );

  const handleInput = useCallback((e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setInput(e.target.value);
    const el = e.target;
    el.style.height = 'auto';
    el.style.height = `${clamp(el.scrollHeight, 56, 256)}px`;
  }, []);

  const showReopen = canReopen(item);
  const showRework = canRework(item);

  return (
    <div className="bg-background px-3 pb-3 pt-1">
      <div
        className={cn(
          'rounded-xl border bg-surface-1 transition-colors',
          intent !== 'ask' ? 'border-accent/40' : 'border-border',
          'focus-within:border-text-3',
        )}
      >
        <textarea
          ref={textareaRef}
          value={input}
          onChange={handleInput}
          onKeyDown={handleKeyDown}
          placeholder={
            intent === 'reopen'
              ? 'Describe what to fix (sends as reopen)...'
              : intent === 'rework'
                ? 'Describe what to redo (fresh worker + new branch)...'
                : 'Ask the advisor about this task...'
          }
          rows={2}
          className="min-h-[52px] max-h-[256px] w-full resize-none border-0 bg-transparent px-3.5 pt-3 pb-0 text-body-sm leading-5 text-text-1 placeholder:text-text-3 focus:outline-none"
        />
        <div className="flex items-center justify-between px-1.5 pb-1.5">
          <div>
            {showReopen || showRework ? (
              <select
                value={intent}
                onChange={(e) => setIntent(e.target.value as 'ask' | 'reopen' | 'rework')}
                className="cursor-pointer appearance-none rounded-md bg-transparent py-1 pr-4 pl-2 text-body-sm text-text-3 hover:text-text-1 focus:outline-none"
                style={{
                  backgroundImage: `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='10' viewBox='0 0 24 24' fill='none' stroke='%23666' stroke-width='2.5' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='m6 9 6 6 6-6'/%3E%3C/svg%3E")`,
                  backgroundRepeat: 'no-repeat',
                  backgroundPosition: 'right 3px center',
                }}
              >
                <option value="ask">Ask</option>
                {showReopen && <option value="reopen">Reopen</option>}
                {showRework && <option value="rework">Rework</option>}
              </select>
            ) : (
              <span className="py-1 pl-2 text-body-sm text-text-4">Ask</span>
            )}
          </div>
          <button
            type="button"
            onClick={handleSubmit}
            disabled={!input.trim() || isPending}
            className={cn(
              'flex h-7 w-7 items-center justify-center rounded-lg transition-all duration-150',
              input.trim() && !isPending
                ? 'bg-text-1 text-background hover:opacity-80'
                : 'text-text-4',
            )}
          >
            {isPending ? <Loader2 size={14} className="animate-spin" /> : <ArrowUp size={14} />}
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Main Feed View ──

interface TaskFeedViewProps {
  item: TaskItem;
}

export function TaskFeedView({ item }: TaskFeedViewProps): React.ReactElement {
  const feedEndRef = useRef<HTMLDivElement>(null);
  const { data: feedData } = useTaskFeed(item.id);
  const advisorMutation = useTaskAdvisor();

  const feedItems = feedData?.feed ?? [];

  // Find the latest clarify_question timestamp so only that one renders interactively.
  const latestClarifyTs = useMemo(() => {
    for (let i = feedItems.length - 1; i >= 0; i--) {
      const fi = feedItems[i];
      if (fi.type === 'timeline' && (fi.data as TimelineEvent).event_type === 'clarify_question') {
        return fi.timestamp;
      }
    }
    return null;
  }, [feedItems]);
  const isLatestClarify = useCallback((ts: string) => ts === latestClarifyTs, [latestClarifyTs]);

  // Auto-scroll: callback ref on the sentinel div scrolls into view when
  // feedItems.length increases. Safe for concurrent mode (no render side-effects).
  const prevCountRef = useRef(feedItems.length);
  const feedEndCallbackRef = useCallback(
    (node: HTMLDivElement | null) => {
      feedEndRef.current = node;
      if (node && feedItems.length > prevCountRef.current) {
        node.scrollIntoView({ behavior: 'smooth' });
      }
      prevCountRef.current = feedItems.length;
    },
    [feedItems.length],
  );

  const handleSend = useCallback(
    (message: string, intent: string) => {
      advisorMutation.mutate({ id: item.id, message, intent });
    },
    [advisorMutation, item.id],
  );

  return (
    <div className="flex h-full flex-col">
      {/* Feed content */}
      <div className="scrollbar-on-hover min-h-0 flex-1 overflow-y-auto">
        {feedItems.length === 0 ? (
          <div className="flex h-full items-center justify-center">
            <div className="text-center text-text-3">
              <Clock size={32} className="mx-auto mb-2 opacity-50" />
              <p className="text-body-sm">Waiting for activity...</p>
            </div>
          </div>
        ) : (
          <div className="py-2">
            {feedItems.map((entry, i) => (
              <FeedBlock
                key={`${entry.type}-${entry.timestamp}-${i}`}
                item={entry}
                task={item}
                isLatestClarify={isLatestClarify}
              />
            ))}
            <div ref={feedEndCallbackRef} />
          </div>
        )}
      </div>

      {/* Advisor input bar */}
      <AdvisorInputBar item={item} onSend={handleSend} isPending={advisorMutation.isPending} />
    </div>
  );
}
