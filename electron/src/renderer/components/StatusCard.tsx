import React, { useState, useCallback } from 'react';
import type { TaskItem, ClarifierQuestion, SessionSummary } from '#renderer/types';
import { answerClarification } from '#renderer/api';
import { useTaskStore } from '#renderer/stores/taskStore';
import { useToastStore } from '#renderer/stores/toastStore';
import log from '#renderer/logger';
import { fmtDuration, getErrorMessage, relativeTime } from '#renderer/utils';
import { PrIcon } from '#renderer/components/TaskIcons';

interface Props {
  item: TaskItem;
  sessions: SessionSummary[];
  /** Structured questions from latest clarify_question timeline event. */
  clarifierQuestions: ClarifierQuestion[] | null;
}

/* ── Variant renderers ── */

function StreamingCard({ item, sessions }: Pick<Props, 'item' | 'sessions'>) {
  const active = sessions.find((s) => s.status === 'running');
  const dur = active ? (active.duration_ms ?? 0) / 1000 : 0;
  const cost = active ? (active.cost_usd ?? 0) : 0;
  return (
    <CardShell color="var(--color-success)">
      <StatusDot color="var(--color-success)" pulse />
      <span className="text-body font-medium" style={{ color: 'var(--color-text-1)' }}>
        Streaming
      </span>
      <Sep />
      <span className="text-caption" style={{ color: 'var(--color-text-2)' }}>
        {item.worker ?? 'Worker'} &middot; {dur > 0 ? fmtDuration(dur) : 'starting'}
        {cost > 0 && ` · $${cost.toFixed(2)}`}
      </span>
    </CardShell>
  );
}

function QueuedCard() {
  return (
    <CardShell color="var(--color-text-4)">
      <span className="text-body" style={{ color: 'var(--color-text-3)' }}>
        &#9719; Queued
      </span>
    </CardShell>
  );
}

function CaptainReviewingCard() {
  return (
    <CardShell color="var(--color-accent)">
      <StatusDot color="var(--color-accent)" pulse />
      <span className="text-body font-medium" style={{ color: 'var(--color-text-1)' }}>
        Captain reviewing
      </span>
    </CardShell>
  );
}

function AwaitingReviewCard({ item }: { item: TaskItem }) {
  return (
    <CardShell color="var(--color-success)">
      <StatusDot color="var(--color-success)" />
      <span className="text-body font-medium" style={{ color: 'var(--color-text-1)' }}>
        Ready for review
      </span>
      {item.pr && (
        <>
          <Sep />
          <span className="text-caption" style={{ color: 'var(--color-text-2)' }}>
            PR {item.pr.replace(/.*\/pull\//, '#')}
          </span>
        </>
      )}
    </CardShell>
  );
}

function EscalatedCard({ item }: { item: TaskItem }) {
  const [expanded, setExpanded] = useState(false);
  const preview = item.escalation_report?.slice(0, 120) ?? '';
  return (
    <CardShell color="var(--color-error)">
      <div className="flex w-full flex-col gap-1">
        <div className="flex items-center gap-2">
          <StatusDot color="var(--color-error)" />
          <span className="text-body font-medium" style={{ color: 'var(--color-text-1)' }}>
            Escalated
          </span>
        </div>
        {preview && (
          <div className="text-caption" style={{ color: 'var(--color-text-2)' }}>
            &ldquo;{expanded ? item.escalation_report : preview}
            {!expanded && (item.escalation_report?.length ?? 0) > 120 ? '...' : ''}
            &rdquo;
            {(item.escalation_report?.length ?? 0) > 120 && (
              <button
                onClick={() => setExpanded((v) => !v)}
                className="ml-1"
                style={{
                  background: 'none',
                  border: 'none',
                  color: 'var(--color-accent)',
                  cursor: 'pointer',
                  padding: 0,
                  fontSize: 'inherit',
                }}
              >
                {expanded ? 'Less' : 'Full report'}
              </button>
            )}
          </div>
        )}
      </div>
    </CardShell>
  );
}

function NeedsClarificationCard({
  taskId,
  questions,
}: {
  taskId: number;
  questions: ClarifierQuestion[];
}) {
  const unanswered = questions.filter((q) => !q.self_answered);
  const selfAnswered = questions.filter((q) => q.self_answered);
  const [answers, setAnswers] = useState<Record<number, string>>({});
  const [pending, setPending] = useState(false);
  const [completed, setCompleted] = useState<string | null>(null);
  const [selfExpanded, setSelfExpanded] = useState(false);
  const taskFetch = useTaskStore((s) => s.fetch);
  const toast = useToastStore.getState;

  const filledCount = unanswered.filter((_, i) => answers[i]?.trim()).length;

  const handleSubmit = useCallback(async () => {
    const payload = unanswered
      .map((q, i) => ({ question: q.question, answer: answers[i]?.trim() || '' }))
      .filter((a) => a.answer.length > 0);
    if (payload.length === 0) return;

    setPending(true);
    try {
      const result = await answerClarification(taskId, payload);
      taskFetch();
      const msgs: Record<string, [variant: 'success' | 'info', msg: string]> = {
        ready: ['success', 'Clarified — task queued'],
        clarifying: ['info', 'Still needs more info'],
        escalate: ['info', 'Escalated to captain review'],
      };
      const [variant, msg] = msgs[result.status] ?? ['success', 'Answer saved'];
      toast().add(variant, msg);
      if (result.status !== 'clarifying') setCompleted(msg);
      else setAnswers({});
    } catch (err) {
      log.warn(`[StatusCard] clarification submit failed for task ${taskId}:`, err);
      toast().add('error', getErrorMessage(err, 'Failed to submit answers'));
    } finally {
      setPending(false);
    }
  }, [answers, unanswered, taskId, taskFetch, toast]);

  if (completed) {
    return (
      <CardShell color="var(--color-success)">
        <span className="text-body font-medium" style={{ color: 'var(--color-success)' }}>
          {completed}
        </span>
      </CardShell>
    );
  }

  return (
    <div
      className="rounded-lg px-4 py-3"
      style={{
        background: 'color-mix(in srgb, var(--color-needs-human) 6%, transparent)',
        border: '1px solid color-mix(in srgb, var(--color-needs-human) 20%, transparent)',
      }}
    >
      <div className="mb-3 flex items-center gap-2">
        <StatusDot color="var(--color-needs-human)" />
        <span className="text-body font-medium" style={{ color: 'var(--color-text-1)' }}>
          Needs your input
        </span>
        <span className="text-caption" style={{ color: 'var(--color-text-3)' }}>
          {unanswered.length} of {questions.length} remaining
        </span>
      </div>

      <div className="space-y-3">
        {unanswered.map((q, i) => (
          <div key={i}>
            <div
              className="mb-1 break-words text-body leading-snug"
              style={{ color: 'var(--color-text-1)' }}
            >
              <span style={{ color: 'var(--color-text-3)' }}>{i + 1}.</span> {q.question}
            </div>
            <textarea
              className="w-full resize-none rounded-md bg-transparent px-3 py-2 text-body leading-snug focus:outline-none"
              style={{
                color: 'var(--color-text-1)',
                border: '1px solid var(--color-border-subtle)',
                background: 'var(--color-surface-2)',
              }}
              rows={1}
              placeholder="Your answer..."
              value={answers[i] ?? ''}
              onChange={(e) => setAnswers((prev) => ({ ...prev, [i]: e.target.value }))}
              disabled={pending}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && e.metaKey && filledCount > 0) {
                  e.preventDefault();
                  handleSubmit();
                }
              }}
            />
          </div>
        ))}
      </div>

      {selfAnswered.length > 0 && (
        <div className="mt-3">
          <button
            onClick={() => setSelfExpanded((v) => !v)}
            className="flex items-center gap-1.5 text-caption"
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
                transform: selfExpanded ? 'rotate(90deg)' : 'none',
              }}
            >
              <path d="M2 1l4 3-4 3V1z" />
            </svg>
            Self-answered ({selfAnswered.length})
          </button>
          {selfExpanded && (
            <div className="mt-2 space-y-2">
              {selfAnswered.map((q, i) => (
                <div key={i} style={{ opacity: 0.7 }}>
                  <div
                    className="break-words text-caption"
                    style={{ color: 'var(--color-text-3)' }}
                  >
                    Q: {q.question}
                  </div>
                  {q.answer && (
                    <div
                      className="mt-0.5 break-words text-caption leading-relaxed"
                      style={{ color: 'var(--color-text-4)' }}
                    >
                      A: {q.answer}
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      <div
        className="mt-3 flex items-center justify-between rounded-lg px-3 py-2"
        style={{
          background: 'var(--color-surface-2)',
          border: '1px solid var(--color-border-subtle)',
        }}
      >
        <span className="text-caption" style={{ color: 'var(--color-text-3)' }}>
          {filledCount} of {unanswered.length} answered
        </span>
        <button
          onClick={handleSubmit}
          disabled={filledCount === 0 || pending}
          className="rounded-md px-4 py-1.5 text-caption font-medium disabled:opacity-40"
          style={{
            background: 'var(--color-accent)',
            color: 'var(--color-bg)',
            border: 'none',
            cursor: filledCount === 0 || pending ? 'default' : 'pointer',
          }}
        >
          {pending ? 'Submitting...' : `Submit (${filledCount})`}
        </button>
      </div>
    </div>
  );
}

function FailedCard({ item }: { item: TaskItem }) {
  return (
    <CardShell color="var(--color-error)">
      <StatusDot color="var(--color-error)" />
      <span className="text-body font-medium" style={{ color: 'var(--color-text-1)' }}>
        {item.status === 'errored' ? 'Failed' : 'Rework'}
      </span>
      <Sep />
      <span className="text-caption" style={{ color: 'var(--color-text-2)' }}>
        {item.intervention_count > 0 && `${item.intervention_count} interventions`}
      </span>
    </CardShell>
  );
}

function MergedCard({ item, sessions }: Pick<Props, 'item' | 'sessions'>) {
  const totalCost = sessions.reduce((s, x) => s + (x.cost_usd ?? 0), 0);
  return (
    <CardShell color="var(--color-text-4)">
      {item.status === 'merged' && <PrIcon state="merged" />}
      <span className="text-body" style={{ color: 'var(--color-text-3)' }}>
        {item.status === 'merged'
          ? 'Merged'
          : item.status === 'canceled'
            ? 'Canceled'
            : 'Completed'}
      </span>
      <Sep />
      <span className="text-caption" style={{ color: 'var(--color-text-3)' }}>
        {item.last_activity_at && relativeTime(item.last_activity_at)}
        {sessions.length > 0 && ` · ${sessions.length} sessions`}
        {totalCost > 0 && ` · $${totalCost.toFixed(2)}`}
      </span>
    </CardShell>
  );
}

/* ── Main export ── */

export function StatusCard({ item, sessions, clarifierQuestions }: Props): React.ReactElement {
  const s = item.status;

  if (s === 'needs-clarification' && clarifierQuestions && clarifierQuestions.length > 0) {
    return <NeedsClarificationCard taskId={item.id} questions={clarifierQuestions} />;
  }
  if (s === 'needs-clarification') {
    return (
      <CardShell color="var(--color-needs-human)">
        <StatusDot color="var(--color-needs-human)" />
        <span className="text-body font-medium" style={{ color: 'var(--color-text-1)' }}>
          Needs your input
        </span>
      </CardShell>
    );
  }

  if (s === 'in-progress' || s === 'clarifying')
    return <StreamingCard item={item} sessions={sessions} />;
  if (s === 'new' || s === 'queued') return <QueuedCard />;
  if (s === 'captain-reviewing' || s === 'captain-merging') return <CaptainReviewingCard />;
  if (s === 'awaiting-review') return <AwaitingReviewCard item={item} />;
  if (s === 'escalated') return <EscalatedCard item={item} />;
  if (s === 'errored' || s === 'rework') return <FailedCard item={item} />;
  if (s === 'handed-off') {
    return (
      <CardShell color="var(--color-text-3)">
        <span className="text-body" style={{ color: 'var(--color-text-3)' }}>
          Handed off
        </span>
      </CardShell>
    );
  }

  // merged, completed-no-pr, canceled
  return <MergedCard item={item} sessions={sessions} />;
}

/* ── Shared primitives ── */

function CardShell({
  color,
  children,
}: {
  color: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <div
      className="flex items-center gap-2 rounded-lg px-4 py-3"
      style={{
        background: `color-mix(in srgb, ${color} 6%, transparent)`,
        border: `1px solid color-mix(in srgb, ${color} 20%, transparent)`,
      }}
    >
      {children}
    </div>
  );
}

function StatusDot({ color, pulse }: { color: string; pulse?: boolean }): React.ReactElement {
  return (
    <span
      className={`inline-block h-2 w-2 shrink-0 rounded-full${pulse ? ' animate-pulse' : ''}`}
      style={{ background: color }}
    />
  );
}

function Sep(): React.ReactElement {
  return (
    <span className="text-caption" style={{ color: 'var(--color-text-4)' }}>
      &middot;
    </span>
  );
}
